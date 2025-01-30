use hello_world::greeter_client::GreeterClient;
use hello_world::HelloRequest;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use regex::Regex;

pub mod hello_world {
    tonic::include_proto!("helloworld");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Shared data structure to store server counts
    let server_counts = Arc::new(Mutex::new(HashMap::new()));

    // Create a vector to store thread handles
    let mut handles = vec![];

    // Create 5 threads
    for j in 0..5 {
        let server_counts = Arc::clone(&server_counts);

        // Spawn a thread
        let handle = tokio::spawn(async move {
            // Connect to the server
            let mut client = GreeterClient::connect("http://[::1]:50051").await.unwrap();

            // Regex to extract the server number
            let server_regex = Regex::new(r"server (\d+)").unwrap();

            // Perform 100K calls
            for i in 0..10000 {
                // if i % 1000 == 0 {
                //     println!("10K requests completed for thread {} {}", j);
                // }
                let request = tonic::Request::new(HelloRequest {
                    name: "Rustacean".to_string(),
                });

                if let Ok(response) = client.say_hello(request).await {
                    let message = response.into_inner().message;

                    // Extract server number using regex
                    if let Some(captures) = server_regex.captures(&message) {
                        if let Some(server_num) = captures.get(1) {
                            let server_num = server_num.as_str().to_string();

                            // Update the count for the server
                            let mut counts = server_counts.lock().await;
                            *counts.entry(server_num).or_insert(0) += 1;
                            if i % 1000 == 0 {
                                println!("1K requests completed for thread {} {:?}", j, counts);
                            }
                        }
                    }
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to finish
    for handle in handles {
        handle.await?;
    }

    // Log the final server counts
    let server_counts = server_counts.lock().await;
    println!("Final server counts: {:?}", *server_counts);

    Ok(())
}
