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

            tracing::info!("Simulating delay of {} ms for server 1", random_delay);
            let reply = HelloReply {
                message: format!("Hello {}! from server 1", request.into_inner().name),
            };
            reply
        });
        self.rif.clone().lock().unwrap().fetch_sub(1, Ordering::SeqCst);
        self.latencies.clone().lock().unwrap().add_latency(macro_response.1.as_nanos());
        // tracing::info!("Added the latency {:?}", self.latencies.clone());
        tracing::info!("Time taken for processing the request is {:?}", macro_response.1);
        Ok(Response::new(macro_response.0))
    }
    async fn get_metrics(&self, _request: Request<Empty>) -> Result<Response<Metric>, Status> {
        println!("Got a request for metrics");
        let rif = self.rif.lock().unwrap().load(Ordering::SeqCst);
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
    let addr = "[::1]:50052".parse()?;
    let greeter = MyGreeter::default();
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber)?;

    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hello_world::greeter_client::GreeterClient;
    use hello_world::{HelloRequest, Empty};
    use tonic::transport::Channel;
    use std::time::Duration;
    use tokio::time::sleep;

    // Helper function to create a gRPC client
    async fn create_client(addr: &str) -> GreeterClient<Channel> {
        GreeterClient::connect(addr.to_string()).await.unwrap()
    }

    // Test the `say_hello` method
    #[tokio::test]
    async fn test_say_hello() {
        // Start the server in a separate task
        let server_handle = tokio::spawn(async {
            let addr = "[::1]:50052".parse().unwrap();
            let greeter = MyGreeter::default();
            Server::builder()
                .add_service(GreeterServer::new(greeter))
                .serve(addr)
                .await
                .unwrap();
        });

        // Wait for the server to start
        sleep(Duration::from_millis(100)).await;

        // Create a client
        let mut client = create_client("http://[::1]:50052").await;

        // Test a valid request
        let request = tonic::Request::new(HelloRequest {
            name: "world".to_string(),
        });
        let response = client.say_hello(request).await.unwrap();
        assert_eq!(response.into_inner().message, "Hello world! from server 1");

        // Test another request
        let request = tonic::Request::new(HelloRequest {
            name: "tonic".to_string(),
        });
        let response = client.say_hello(request).await.unwrap();
        assert_eq!(response.into_inner().message, "Hello tonic! from server 1");

        // Shutdown the server
        server_handle.abort();
    }

    // Test the `get_metrics` method
    #[tokio::test]
    async fn test_get_metrics() {
        // Start the server in a separate task
        let server_handle = tokio::spawn(async {
            let addr = "[::1]:50053".parse().unwrap();
            let greeter = MyGreeter::default();
            Server::builder()
                .add_service(GreeterServer::new(greeter))
                .serve(addr)
                .await
                .unwrap();
        });

        // Wait for the server to start
        sleep(Duration::from_millis(100)).await;

        // Create a client
        let mut client = create_client("http://[::1]:50053").await;

        // Send a few requests to populate metrics
        for _ in 0..5 {
            let request = tonic::Request::new(HelloRequest {
                name: "world".to_string(),
            });
            client.say_hello(request).await.unwrap();
        }

        // Fetch metrics
        let request = tonic::Request::new(Empty {});
        let response = client.get_metrics(request).await.unwrap();
        let metrics = response.into_inner();

        // Validate metrics
        assert_eq!(metrics.rif, 0); // All requests should be processed by now
        assert!(metrics.latency > 0); // Latency should be recorded

        // Shutdown the server
        server_handle.abort();
    }

    // Test concurrent requests
    #[tokio::test]
    async fn test_concurrent_requests() {
        // Start the server in a separate task
        let server_handle = tokio::spawn(async {
            let addr = "[::1]:50054".parse().unwrap();
            let greeter = MyGreeter::default();
            Server::builder()
                .add_service(GreeterServer::new(greeter))
                .serve(addr)
                .await
                .unwrap();
        });

        // Wait for the server to start
        sleep(Duration::from_millis(100)).await;

        // Create multiple clients to simulate concurrent requests
        let mut client1 = create_client("http://[::1]:50054").await;
        let mut client2 = create_client("http://[::1]:50054").await;

        // Send requests concurrently
        let handle1 = tokio::spawn(async move {
            let request = tonic::Request::new(HelloRequest {
                name: "client1".to_string(),
            });
            client1.say_hello(request).await.unwrap()
        });

        let handle2 = tokio::spawn(async move {
            let request = tonic::Request::new(HelloRequest {
                name: "client2".to_string(),
            });
            client2.say_hello(request).await.unwrap()
        });

        // Wait for both requests to complete
        let response1 = handle1.await.unwrap();
        let response2 = handle2.await.unwrap();

        // Validate responses
        assert_eq!(response1.into_inner().message, "Hello client1! from server 1");
        assert_eq!(response2.into_inner().message, "Hello client2! from server 1");

        // Shutdown the server
        server_handle.abort();
    }

    // Test metrics after multiple requests
    #[tokio::test]
    async fn test_metrics_after_multiple_requests() {
        // Start the server in a separate task
        let server_handle = tokio::spawn(async {
            let addr = "[::1]:50055".parse().unwrap();
            let greeter = MyGreeter::default();
            Server::builder()
                .add_service(GreeterServer::new(greeter))
                .serve(addr)
                .await
                .unwrap();
        });

        // Wait for the server to start
        sleep(Duration::from_millis(100)).await;

        // Create a client
        let mut client = create_client("http://[::1]:50055").await;

        // Send multiple requests
        for i in 0..10 {
            let request = tonic::Request::new(HelloRequest {
                name: format!("world{}", i),
            });
            client.say_hello(request).await.unwrap();
        }

        // Fetch metrics
        let request = tonic::Request::new(Empty {});
        let response = client.get_metrics(request).await.unwrap();
        let metrics = response.into_inner();

        // Validate metrics
        assert_eq!(metrics.rif, 0); // All requests should be processed by now
        assert!(metrics.latency > 0); // Latency should be recorded

        // Shutdown the server
        server_handle.abort();
    }

    // Test server behavior under high load
    #[tokio::test]
    async fn test_high_load() {
        // Start the server in a separate task
        let server_handle = tokio::spawn(async {
            let addr = "[::1]:50056".parse().unwrap();
            let greeter = MyGreeter::default();
            Server::builder()
                .add_service(GreeterServer::new(greeter))
                .serve(addr)
                .await
                .unwrap();
        });

        // Wait for the server to start
        sleep(Duration::from_millis(100)).await;

        // Create multiple clients to simulate high load
        let mut handles = vec![];
        for i in 0..50 {
            let mut client = create_client("http://[::1]:50056").await;
            let handle = tokio::spawn(async move {
                let request = tonic::Request::new(HelloRequest {
                    name: format!("client{}", i),
                });
                client.say_hello(request).await.unwrap()
            });
            handles.push(handle);
        }

        // Wait for all requests to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Fetch metrics
        let mut client = create_client("http://[::1]:50056").await;
        let request = tonic::Request::new(Empty {});
        let response = client.get_metrics(request).await.unwrap();
        let metrics = response.into_inner();

        // Validate metrics
        assert_eq!(metrics.rif, 0); // All requests should be processed by now
        assert!(metrics.latency > 0); // Latency should be recorded

        // Shutdown the server
        server_handle.abort();
    }
}
