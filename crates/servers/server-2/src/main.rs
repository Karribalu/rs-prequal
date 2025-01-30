use hello_world::greeter_server::{Greeter, GreeterServer};
use hello_world::{Empty, HelloReply, HelloRequest, Metric};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use rand::Rng;
use tokio::time::sleep;
use tonic::{transport::Server, Request, Response, Status};
use utils::measure_time;
use utils::medianfinder::MedianFinder;


pub mod hello_world {
    tonic::include_proto!("helloworld");
}
#[derive(Debug, Default)]
pub struct MyGreeter {
    pub rif: Arc<Mutex<AtomicU32>>,
    pub latencies: Arc<Mutex<MedianFinder>>,
}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(&self, request: Request<HelloRequest>) -> Result<Response<HelloReply>, Status> {
        self.rif.clone().lock().unwrap().fetch_add(1, Ordering::Acquire);
        println!("Got a request: {:?}", request);
        let macro_response = measure_time!({
             // Generate a random delay between 100ms and 1s
            let random_delay = {
                let mut rng = rand::thread_rng();
                rng.gen_range(0..=10) // Generate milliseconds
            };
            sleep(Duration::from_millis(random_delay as u64)).await;

            tracing::info!("Simulating delay of {} ms for server 2", random_delay);
            let reply = HelloReply {
                message: format!("Hello {}! from server 2", request.into_inner().name),
            };
            reply
        });
        self.rif.clone().lock().unwrap().fetch_sub(1, Ordering::Acquire);
        self.latencies.clone().lock().unwrap().add_latency(macro_response.1.as_nanos());
        // tracing::info!("Added the latency {:?}", self.latencies.clone());
        tracing::info!("Time taken for processing the request is {:?}", macro_response.1);

        Ok(Response::new(macro_response.0))
    }
    async fn get_metrics(&self, _request: Request<Empty>) -> Result<Response<Metric>, Status> {
        println!("Got a request for metrics");
        let rif = self.rif.lock().unwrap().load(Ordering::Acquire);
        let latency_op = self.latencies.lock().unwrap().find_median();
        let mut latency = 0;
        if latency_op.is_some() {
            latency = latency_op.unwrap();
        }
        let reply = Metric {
            rif,
            latency: latency as u64,
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50053".parse()?;
    let greeter = MyGreeter::default();
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;

    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve(addr)
        .await?;

    Ok(())
}
