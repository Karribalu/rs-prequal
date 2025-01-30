# Prequal PoC: Asynchronous Probing in Load Balancing

## Overview

This repository contains a **Proof of Concept (PoC)** implementation of the **Prequal** load balancing approach, specifically focusing on **asynchronous probing**. The implementation is based on the concepts outlined in the **"Load is Not What You Should Balance: Introducing Prequal"** paper presented at **NSDI 2024** by Google.
This load balancing paradigm is being used in YouTube and other production systems at Google to run at much higher utilization.

## Key Features

- Implements **asynchronous probing** to select the best server replica.
- Uses **gRPC** for communication between the load balancer and backend servers.
- Built using **Rust**, **Tokio**, and **tonic** for asynchronous execution.
- Dynamically selects a backend server based on **requests-in-flight (RIF)** and estimated latency.
- Avoids traditional CPU load balancing in favor of real-time request latency reduction.

## Project Structure

```
prequal/
│── Cargo.toml            # Rust workspace configuration
│── proto/helloworld.proto  # gRPC service definitions
│── crates/
│   ├── load-balancer/    # Load balancer implementation
│   ├── clients/          # Client implementations
│   │   ├── client-1/
│   │   ├── client-2/
│   │   ├── client-3/
│   ├── servers/          # Backend server implementations
│   │   ├── server-1/
│   │   ├── server-2/
│   │   ├── server-3/
│   ├── utils/            # Utility functions (latency calculations, median finder, etc.)
```

## Installation

### Prerequisites

- **Rust** (Latest stable version)
- **Cargo** (Rust package manager)
- **Tokio** (Asynchronous runtime)
- **tonic** (gRPC library for Rust)
- **Protobuf Compiler** (For generating gRPC bindings)

### Clone the Repository

```sh
git clone https://github.com/karribalu/rs-prequal.git
cd rs-prequal
```

### Build the Project

```sh
cargo build --release
```

## Running the Load Balancer

Start the gRPC load balancer:

```sh
cargo run -p load-balancer
```

## Running Backend Servers

Start multiple backend gRPC servers:

```sh
cargo run -p server-1
cargo run -p server-2
cargo run -p server-3
```

## Running a gRPC Client

Once the load balancer and backend servers are running, you can send gRPC requests using a client:

```sh
cargo run -p client-1
```

## Configuration

The list of backend servers is defined in the `.env` file:

```
SERVER_URLS=http://127.0.0.1:50051,http://127.0.0.1:50052,http://127.0.0.1:50053
```

## How It Works

1. The **load balancer** receives incoming gRPC requests.
2. It **asynchronously probes** multiple backend servers to measure latency and requests-in-flight (RIF).
3. The best server is selected based on **median latency and RIF**.
4. The request is forwarded, and the response is returned to the client.

## Future Enhancements

- Implement **hot-cold lexicographic (HCL) rule** for better load balancing decisions.
- Add **benchmarking tests** to compare performance against WRR-based load balancers.
- Improve **error handling** and fault tolerance mechanisms.
- Introduce **health checks** for better server selection.

## References

- [Prequal Paper - NSDI 2024](https://www.usenix.org/conference/nsdi24/presentation/wydrowski)
- [Rust gRPC (tonic) Documentation](https://docs.rs/tonic/latest/tonic/)
- [Tokio Asynchronous Runtime](https://tokio.rs/)

---

This PoC was implemented by **Balasubramanyam** as an exploration of Prequal's **asynchronous probing** mechanism. 🚀

