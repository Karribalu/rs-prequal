use crate::hello_world::greeter_client::GreeterClient;
use crate::hello_world::{Empty, Metric};
use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, SeedableRng};
use std::cmp::Ordering;
use std::fmt::{write, Debug, Display, Formatter};
use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{atomic, Arc};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::oneshot;
use tokio::sync::{Mutex, MutexGuard};
use tokio::task;
use tokio::time::interval;
use tonic::transport::{Channel, Error};
use tonic::{transport::Server, Code, Request, Response, Status};
use utils::medianfinder::MedianFinder;

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
#[derive(Debug, Default)]
pub struct LoadBalancer {
    pub clients: Vec<Client>,
    pub probe_pool: Vec<Probe>,
}
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct Probe {
    pub server: String,
    pub rif: u32,
    pub latency: u64,
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

impl PartialOrd<Self> for Probe {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            Option::from(Ordering::Equal)
        } else if self.rif == other.rif {
            if self.latency < other.latency {
                Option::from(Ordering::Less)
            } else {
                Option::from(Ordering::Greater)
            }
        } else if self.rif < other.rif {
            Option::from(Ordering::Less)
        } else {
            Option::from(Ordering::Greater)
        }
    }
}

impl Ord for Probe {
    fn cmp(&self, other: &Self) -> Ordering {
        if self == other {
            Ordering::Equal
        } else if self.rif == other.rif {
            if self.latency < other.latency {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        } else if self.rif < other.rif {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
}
impl LoadBalancer {
    pub fn new() -> Self {
        Self {
            clients: vec![],
            probe_pool: vec![],
        }
    }
    /**

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
    */
    pub fn get_server(&mut self) -> Result<&mut GreeterClient<Channel>, LoadBalancerError> {
        // TODO: Implement the efficient server selection from the probe pool
        let first_probe = &self.probe_pool[0].server;
        for client in &mut self.clients {
            if client.client_add.eq(first_probe) && client.is_active.load(SeqCst) {
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
                        // Deletes the existing probe if any for this server
                        self.probe_pool.retain(|x| !x.server.eq(&server.client_add));

                        self.probe_pool.push(Probe {
                            server: server.client_add.clone(),
                            rif: inner.rif,
                            latency: inner.latency,
                        });
                        self.probe_pool.sort();
                        tracing::info!("pool after sorting {:?}", self.probe_pool);
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
        match self.load_balancer.clone().lock().await.get_server() {
            Ok(server) => {
                let response = server.say_hello(request).await;
                response
            }
            Err(error) => {
                tracing::error!(%error, "Internal error while getting the best server for the request {:?} ", request);
                Err(Status::new(
                    Code::Internal,
                    "Internal error while getting the best server",
                ))
            }
        }
    }
    /**
    This function should return the RIF and the median of latencies
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
    let addr = "[::1]:50051".parse()?;
    let mut greeter = MyGreeter::default();
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;
    let load_balancer = Arc::new(Mutex::new(LoadBalancer::default()));
    initialise_load_balancer(load_balancer.clone()).await;
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
async fn initialise_load_balancer(balancer: Arc<Mutex<LoadBalancer>>) {
    let servers = vec![
        String::from("http://[::1]:50052"),
        String::from("http://[::1]:50053"),
        String::from("http://[::1]:50054"),
    ];
    for server in &servers {
        let res = balancer.lock().await.add_client(server.clone()).await;
        match res {
            Ok(_) => {
                tracing::info!("Added the client {:?}", res);
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
    let mut interval = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Perform some background work
                println!("Background task is running...");
                let mut balancer = load_balancer.lock().await;
                for server in &balancer.clients{
                    if !&server.is_active.load(atomic::Ordering::Acquire) {
                        tracing::info!("An inactive server is found, Trying to reconnect");
                        // Try to connect and update the is_active if successful
                         if let client = GreeterClient::connect(server.client_add.to_string()).await {
                            match client {
                                Ok(client) => {
                                    server.is_active.clone().store(true, atomic::Ordering::Release);
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
