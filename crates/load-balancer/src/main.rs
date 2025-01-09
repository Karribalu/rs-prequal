mod server_impl;

use rand::seq::SliceRandom;
use rand::thread_rng;
use thiserror::Error;
use tonic::{transport::Server, Request, Response, Status};
use tonic::transport::{Channel, Error};
use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{HelloReply, HelloRequest};
use crate::hello_world::greeter_client::GreeterClient;
const PROBE_POOL_SIZE: usize = 2;
pub mod hello_world {
    tonic::include_proto!("helloworld");
}
#[derive(Debug, Default)]
pub struct MyGreeter {}
pub struct Client {
    pub client_add: String,
    pub client: GreeterClient<Channel>,
}
pub struct LoadBalancer {
    pub clients: Vec<Client>,
    pub probe_pool: Vec<Probe>,
}
pub struct Probe {
    pub server: String,
    pub rif: u32,
    pub latency: u32,
}
#[derive(Error, Debug)]
pub enum LoadBalancerError {
    #[error("The route`{0}` is not found to delete")]
    RouteNotFoundToDelete(String),
    UnableToEstablishConnectivity(String),
}
impl LoadBalancer {
    pub fn new() -> Self {
        Self {
            clients: vec![],
            probe_pool: vec![],
        }
    }
    pub async fn add_client(&mut self, addr: String) -> Result<(), LoadBalancerError> {
        if let client = GreeterClient::connect("http://[::1]:50051").await {
            match client {
                Ok(client) => {
                    self.clients.push(Client {
                        client_add: addr,
                        client,
                    });
                }
                Err(error) => {
                    return Err(LoadBalancerError::UnableToEstablishConnectivity(error.to_string()));
                }
            }
        }

        Ok(())
    }
    pub async fn remove_client(&mut self, addr: String) -> Result<(), LoadBalancerError> {
        let mut idx = -1;
        for i in 0..self.clients.len() {
            if self.clients[i].client_add.eq(&addr) {
                idx = i;
            }
        }
        if idx != -1 {
            self.clients.remove(idx);
        }
        Ok(())
    }
    pub fn get_server(&mut self) -> Result<GreeterClient<Channel>, LoadBalancerError> {}

    fn probe_servers(&self) {
        // Probes
        let mut rng = thread_rng();
        let probing_servers: Vec<&String> = self.clients.choose_multiple(&mut rng, PROBE_POOL_SIZE).collect();
    }
}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(&self, request: Request<HelloRequest>) -> Result<Response<HelloReply>, Status> {
        println!("Got a request: {:?}", request);

        let reply = HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };

        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let greeter = MyGreeter::default();

    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve(addr)
        .await?;

    Ok(())
}
