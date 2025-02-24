
use crate::hello_world::greeter_client::GreeterClient;
use crate::hello_world::{Empty, Metric};
use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, SeedableRng};
use serde::Deserialize;
use std::cmp::Ordering;
use std::env;
use std::fmt::{write, Debug, Display, Formatter};
use std::fs::File;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::atomic::Ordering::{Acquire, Release, SeqCst};
use std::sync::{atomic, Arc};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::oneshot;
use tokio::sync::{Mutex, MutexGuard};
use tokio::task;
use tokio::time::interval;
use tonic::transport::{Channel, Error};
use tonic::{transport::Server, Code, Request, Response, Status};
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use utils::medianfinder::MedianFinder;

#[derive(Deserialize, Debug, Default, Clone)]
struct Config {
    server_urls: String,
    q_rif: f32,
}
const PROBE_POOL_SIZE: usize = 2;
pub mod hello_world {
    tonic::include_proto!("helloworld");
}
#[derive(Debug, Default)]
pub struct MyGreeter {
    load_balancer: Arc<Mutex<LoadBalancer>>,
}
#[derive(Debug, Clone)]
pub struct Client {
    pub client_add: String,
    pub client: GreeterClient<Channel>,
    pub is_active: Arc<AtomicBool>,
}
#[derive(Debug, Default, Clone)]
pub struct Probe {
    pub server: String,
    pub rif: u32,
    pub latency: u64,
    pub times_used: Arc<AtomicU32>,
    pub normalized_rif: f32, // Used for finding the worst probe based qRIF
}
#[derive(Debug, Default)]
pub struct LoadBalancer {
    pub clients: Vec<Client>,
    pub probe_pool: Vec<Probe>,
    pub max_rif: Arc<AtomicU32>,
    pub config: Config,
}

#[derive(Error, Debug)]
pub enum LoadBalancerError {
    #[error("The route`{0}` is not found to delete")]
    RouteNotFoundToDelete(String),
    #[error("Unable to establish the connectivity `{0}`")]
    UnableToEstablishConnectivity(String),
    #[error("Unable to find the server for best probe")]
    NoProbeFound,
}
impl Display for Metric {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "(rif: {} latency: {})", self.rif, self.latency)
    }
}

impl LoadBalancer {
    pub fn new(config: Config) -> Self {
        Self {
            clients: vec![],
            probe_pool: vec![],
            max_rif: Arc::new(AtomicU32::new(0)),
            config,
        }
    }
    pub fn is_probe_hot(&self, probe: &Probe) -> bool {
        probe.normalized_rif >= (self.config.q_rif)
    }
    /**
    Takes a server address starting with http or https and adds in the clients
    */
    pub async fn add_client(&mut self, addr: String) -> Result<(), LoadBalancerError> {
        if let client = GreeterClient::connect(addr.clone()).await {
            match client {
                Ok(client) => {
                    self.clients.push(Client {
                        client_add: addr.clone(),
                        client,
                        is_active: Arc::new(AtomicBool::new(true)),
                    });
                }
                Err(error) => {
                    return Err(LoadBalancerError::UnableToEstablishConnectivity(
                        error.to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
    /**
    This function is to remove any un-registered clients from the clients vector
    */
    pub async fn remove_client(&mut self, addr: String) -> Result<(), LoadBalancerError> {
        let mut idx = usize::MAX;
        for i in 0..self.clients.len() {
            if self.clients[i].client_add.eq(&addr) {
                idx = i;
            }
        }
        if idx != usize::MAX {
            self.clients.remove(idx);
        }
        Ok(())
    }
    /**
    This function has to determine the best server to chose from the existing probe bool
    Separates the hot and cold servers based on the normalised RIF

    */
    pub fn get_server(&mut self) -> Result<&mut GreeterClient<Channel>, LoadBalancerError> {
        let cold_servers = self.probe_pool.iter()
            .filter(|item| !self.is_probe_hot(*item))
            .collect::<Vec<&Probe>>();
        let mut best_probe;
        if cold_servers.is_empty() {
            best_probe = cold_servers[0];
            // All servers are hot find the one with less RIF
            for i in 1..cold_servers.len() {
                if cold_servers[i].rif < best_probe.rif {
                    best_probe = cold_servers[i];
                }
            }
        } else {
            best_probe = &self.probe_pool[0];
            // find the one with less latency
            for i in 1..self.probe_pool.len() {
                if self.probe_pool[i].latency < best_probe.latency {
                    best_probe = &self.probe_pool[i];
                }
            }
        }
        for client in &mut self.clients {
            if client.client_add.eq(&best_probe.server) && client.is_active.load(SeqCst) {
                best_probe.times_used.fetch_add(1, Acquire);
                return Ok(&mut client.client);
            }
        }
        tracing::error!("No server is found to get");
        Err(LoadBalancerError::NoProbeFound)
    }
    /**
    This function can be called by a job to frequently update the probe pool.
    This selects random servers of SIZE provided by PROBE_POOL_SIZE and gets the metrics from the servers and updates them.
    */
    pub async fn probe_servers(&mut self) {
        // Probes
        let mut rng = StdRng::from_entropy();
        let probing_servers: Vec<Client> = self
            .clients
            .choose_multiple(&mut rng, PROBE_POOL_SIZE)
            .cloned()
            .collect();
        for mut server in probing_servers {
            if let response = server.client.get_metrics(Empty {}).await {
                match response {
                    Ok(metric) => {
                        let inner = metric.into_inner();
                        if inner.rif > self.max_rif.load(Acquire) {
                            tracing::info!("A new max rif is found {}", inner.rif);
                            self.max_rif.store(inner.rif, Release);

                            // When max_rif changes all the probes normalization should change
                            self.probe_pool = self.probe_pool.iter().map(|item| {
                                let mut new_probe = item.clone();
                                new_probe.normalized_rif = (item.rif as f32 / self.max_rif.load(Acquire) as f32);
                                new_probe
                            }).collect::<Vec<Probe>>();
                        }
                        // Deletes the existing probe if any for this server
                        self.probe_pool.retain(|x| !x.server.eq(&server.client_add));
                        // We are multiplying the normalized probe by 10 because Rust is bad at maintaining floats
                        let normalized_rif = inner.rif as f32 / self.max_rif.load(Acquire) as f32;

                        self.probe_pool.push(Probe {
                            server: server.client_add.clone(),
                            rif: inner.rif,
                            latency: inner.latency,
                            times_used: Arc::new(AtomicU32::new(0)),
                            normalized_rif,
                        });
                        tracing::info!("pool after the probe {:?}", self.probe_pool);
                        tracing::info! {
                            %inner,
                            "Received the metric response"
                        }
                    }
                    Err(status) => {
                        if status.code() != Code::Unavailable {
                            tracing::error!("Server is not available for probing {:?}", server);
                        }
                        // Deletes the existing probe if any for this server
                        self.probe_pool.retain(|x| !x.server.eq(&server.client_add));
                        let element = self
                            .clients
                            .iter()
                            .find(|item| item.client_add.eq(&server.client_add));
                        if element.is_some() {
                            element.unwrap().is_active.clone().store(false, SeqCst);
                            tracing::info!(
                                "The server seems to be not active, Marking is_active false {:?}",
                                element
                            );
                        }
                        tracing::error! {
                            %status,
                            "Failed to receive the response for probe "
                        }
                    }
                }
            }
        }
    }
}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    /**
    It has to find the best server to serve request
    Update the RIF and Latencies of the requests
    */
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        let mut lb = self.load_balancer.lock();
        let mut lb = lb.await;

        // Extract information needed immutably before making a mutable borrow
        let probe_pool_snapshot = lb.probe_pool.clone();

        match lb.get_server() {
            Ok(server) => {
                tracing::info!("Diverting the call to the server: {:?}", probe_pool_snapshot);
                let response = server.say_hello(request).await;
                response
            }
            Err(error) => {
                tracing::error!(%error, "Internal error while getting the best server for the request {:?}", request);
                Err(Status::new(
                    Code::Internal,
                    "Internal error while getting the best server",
                ))
            }
        }
    }
    /**
    This function should return the RIF and the median of latencies
    We won't be using this anywhere!
    */
    async fn get_metrics(&self, _request: Request<Empty>) -> Result<Response<Metric>, Status> {
        // Create a longer-lived binding for the cloned Arc
        let cloned_lb = self.load_balancer.clone();

        // Lock the mutex
        let mut load_balancer = cloned_lb.lock().await;
        // Access and modify the load balance's probe pool
        let pool = &mut load_balancer.probe_pool;

        let mut median_lat = pool[pool.len() / 2].latency;
        let mut median_rif = pool[pool.len() / 2].rif;
        if pool.len() % 2 == 0 {
            median_lat = (pool[pool.len() / 2].latency + &pool[(pool.len() / 2) + 1].latency) / 2;
            median_rif = (pool[pool.len() / 2].rif + &pool[(pool.len() / 2) + 1].rif) / 2;
        }
        Ok(Response::new(Metric {
            rif: median_rif,
            latency: median_lat,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let config = envy::from_env::<Config>().expect("Environment config must be set");

    let addr = "[::1]:50051".parse()?;
    let mut greeter = MyGreeter::default();
    let subscriber = tracing_subscriber::FmtSubscriber::new();

    tracing::subscriber::set_global_default(subscriber)?;
    let load_balancer = Arc::new(Mutex::new(LoadBalancer::new(config.clone())));
    tracing::info!("starting the load balancer with initial config {:?}", &config);
    let server_urls = config.server_urls.split(",").map(|item| item.to_string()).collect::<Vec<String>>();
    initialise_load_balancer(load_balancer.clone(), server_urls).await;
    let (mut shutdown_tx, shutdown_rx) = oneshot::channel();
    let background_task = task::spawn(background_process(shutdown_rx, load_balancer.clone()));
    greeter.load_balancer = load_balancer;

    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve_with_shutdown(addr, async {
            // Wait for the shutdown signal
            shutdown_tx.closed().await;
        })
        .await?;

    // Wait for the background task to finish
    background_task.await?;
    Ok(())
}
/**
This function takes care of initialising the clients defined the config
TODO: Add more servers
*/
async fn initialise_load_balancer(balancer: Arc<Mutex<LoadBalancer>>, server_urls: Vec<String>) {
    for server in &server_urls {
        let res = balancer.lock().await.add_client(server.clone()).await;
        match res {
            Ok(_) => {
                tracing::info!("Added the client {:?}", server);
            }
            Err(error) => {
                tracing::info!(
                    "Something wrong happened while connecting to the server {:?}, Error: {:?}",
                    &server,
                    error
                );
            }
        }
    }
}


/**
1. Finds the in active servers and tries to connect
2. Probes the random server to update the metrics
*/
async fn background_process<'a>(
    mut shutdown_signal: oneshot::Receiver<()>,
    mut load_balancer: Arc<Mutex<LoadBalancer>>,
) {
    let mut interval = interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                println!("Probing the servers...");
                let mut balancer = load_balancer.lock().await;
                for server in &balancer.clients {
                    if !&server.is_active.load(Acquire) {
                        tracing::info!("An inactive server is found, Trying to reconnect");
                        // Try to connect and update the is_active if successful
                         if let client = GreeterClient::connect(server.client_add.to_string()).await {
                            match client {
                                Ok(client) => {
                                    server.is_active.clone().store(true, Release);
                                }
                                Err(error) => {
                                    tracing::error!("Unable to contact the server while ticking {:?}", server);
                                }
                            }
                        }
                    }
                }
                balancer.probe_servers().await;
            }
            _ = &mut shutdown_signal => {
                // Clean up before exiting
                println!("Background task received shutdown signal.");
                break;
            }
        }
    }
}
