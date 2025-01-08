use hello_world::greeter_client::GreeterClient;
use hello_world::HelloRequest;


pub mod hello_world {
    tonic::include_proto!("helloworld");
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the server
    let mut client = GreeterClient::connect("http://[::1]:50051").await?;

    // Create a request
    let request = tonic::Request::new(HelloRequest {
        name: "Rustacean".to_string(),
    });

    // Send the request and receive a response
    let response = client.say_hello(request).await?;

    println!("Response from server: {:?}", response.into_inner().message);

    Ok(())
}
