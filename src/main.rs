use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use tokio::main;
use std::net::Ipv4Addr;
use http::header::{HeaderValue, HOST};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::{protocol::Message, client::IntoClientRequest};
use futures_util::{StreamExt, SinkExt};
use url::Url;
use native_tls::TlsConnector;
use tokio_native_tls::TlsConnector as TokioTlsConnector;
use std::net::{SocketAddr, IpAddr};
use tokio::net::TcpStream;

#[derive(Parser, Debug)]
#[clap(version = "0.2", about = "Opinionated CLI tool to hammer the data out of blockchain via WebSockets.", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Fetch {
        endpoint: String,
        block_number: Option<String>,
        #[clap(short, long, help = "Specify an IPv4 address to manually resolve the endpoint, bypassing DNS.")]
        resolve: Option<Ipv4Addr>,
    },
    Mmr {
        endpoint: String,
        block_numbers: Option<Vec<u64>>,
        #[clap(short, long, help = "Specify an IPv4 address to manually resolve the endpoint, bypassing DNS.")]
        resolve: Option<Ipv4Addr>,
    }
}

#[main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Fetch { endpoint, block_number, resolve } => {
            if let Err(e) = fetch_block(&endpoint, block_number.as_deref(), resolve.as_ref()).await {
                eprintln!("Error: {}", e);
            }
        }
        Commands::Mmr { endpoint, block_numbers, resolve } => {
            if let Err(e) = get_mmr_proof(&endpoint, block_numbers, resolve.as_ref()).await {
                eprintln!("Error: {}", e);
            }
        }
    }
}

async fn decimal_to_hexadecimal(decimal_str: &str) -> Result<String, std::num::ParseIntError> {
    let decimal = decimal_str.parse::<u64>()?;
    Ok(format!("{:#x}", decimal))
}

async fn identify_if_hexadecimal_or_decimal(block_number: Option<&str>) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if let Some(number) = block_number {
        if number.starts_with("0x") {
            Ok(Some(number.to_string()))
        } else {
            Ok(Some(decimal_to_hexadecimal(number).await?))
        }
    } else {
        Ok(None)
    }
}

async fn custom_dns_connect(endpoint: &str, dns_override: Option<Ipv4Addr>) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Box<dyn std::error::Error>> {
    let url = Url::parse(endpoint)?;
    let addr = if let Some(ip) = dns_override {
        SocketAddr::new(IpAddr::V4(ip), url.port_or_known_default().ok_or("Unknown port for the URL scheme")?)
    } else {
        let host = url.host_str().ok_or("Missing host in URL")?;
        format!("{}:{}", host, url.port_or_known_default().unwrap_or(443)).parse::<SocketAddr>()?
    };

    let tcp_stream = TcpStream::connect(addr).await?;
    let tls_connector = TlsConnector::builder().danger_accept_invalid_certs(true).build()?;
    let tokio_tls_connector = TokioTlsConnector::from(tls_connector);
    let tls_stream = tokio_tls_connector.connect(url.host_str().unwrap_or(""), tcp_stream).await?;
    let maybe_tls_stream = MaybeTlsStream::NativeTls(tls_stream);

    let mut request = url.clone().into_client_request()?;
    request.headers_mut().insert(HOST, HeaderValue::from_str(url.host_str().unwrap())?);

    let (socket, _) = tokio_tungstenite::client_async(request, maybe_tls_stream).await?;
    Ok(socket)
}

async fn fetch_block(endpoint: &str, block_number: Option<&str>, ipv4: Option<&Ipv4Addr>) -> Result<(), Box<dyn std::error::Error>> {
    // Convert block number to hexadecimal if necessary
    let formatted_block_number = identify_if_hexadecimal_or_decimal(block_number).await?;
    
    // Establish WebSocket connection, with optional DNS override
    let mut socket = if let Some(ip) = ipv4 {
        custom_dns_connect(endpoint, Some(*ip)).await?
    } else {
        let (socket, _) = connect_async(endpoint).await?;
        socket
    };

    // Construct the batch request JSON
    let batch_request = json!([
        { "jsonrpc": "2.0", "id": "1", "method": "system_version", "params": [] },
        { "jsonrpc": "2.0", "id": "2", "method": "system_name", "params": [] },
        { "jsonrpc": "2.0", "id": "3", "method": "system_chain", "params": [] },
        { "jsonrpc": "2.0", "id": "4", "method": "system_health", "params": [] },
        { "jsonrpc": "2.0", "id": "5", "method": if formatted_block_number.is_some() { "chain_getBlockHash" } else { "chain_getHead" }, "params": [formatted_block_number] },
        { "jsonrpc": "2.0", "id": "6", "method": "chain_getFinalizedHead", "params": [] },
        { "jsonrpc": "2.0", "id": "7", "method": "state_getRuntimeVersion", "params": [] },
        { "jsonrpc": "2.0", "id": "8", "method": "system_peers", "params": [] },
        { "jsonrpc": "2.0", "id": "9", "method": "system_syncState", "params": [] }
    ]);

    // Send the batch request
    socket.send(Message::Text(batch_request.to_string())).await?;

    // Initialize response storage
    let mut version = None;
    let mut node_name = None;
    let mut node_chain = None;
    let mut node_health = None;
    let mut block_hash = None;
    let mut finalized_head = None;
//    let mut runtime_version = None;
    let mut peers = None;
    let mut sync_state = None;

    // Read and process responses
    while version.is_none() || node_name.is_none() || node_chain.is_none() || node_health.is_none() || block_hash.is_none() ||
          finalized_head.is_none() /*|| runtime_version.is_none() */ || peers.is_none() || sync_state.is_none() {
        let message = socket.next().await.ok_or("Connection closed before receiving response")??;
        if let Message::Text(text) = message {
            let responses: Vec<Value> = serde_json::from_str(&text)?;
            for response in responses {
                match response["id"].as_str() {
                    Some("1") => version = Some(response["result"].as_str().unwrap_or_default().to_string()),
                    Some("2") => node_name = Some(response["result"].as_str().unwrap_or_default().to_string()),
                    Some("3") => node_chain = Some(response["result"].as_str().unwrap_or_default().to_string()),
                    Some("4") => node_health = Some(response["result"].clone()),
                    Some("5") => block_hash = Some(response["result"].as_str().unwrap_or_default().to_string()),
                    Some("6") => finalized_head = Some(response["result"].as_str().unwrap_or_default().to_string()),
//                    Some("7") => runtime_version = Some(response["result"].clone()),
                    Some("8") => peers = Some(response["result"].clone()),
                    Some("9") => sync_state = Some(response["result"].clone()),
                    _ => {}
                }
            }
        }
    }

    // Unwrap the collected responses
    let version = version.ok_or("Failed to fetch version")?;
    let node_name = node_name.ok_or("Failed to fetch node name")?;
    let node_chain = node_chain.ok_or("Failed to fetch node chain")?;
    let node_health = node_health.ok_or("Failed to fetch node health")?;
    let block_hash = block_hash.ok_or("Failed to fetch block hash")?;
    let finalized_head = finalized_head.ok_or("Failed to fetch finalized head")?;
//    let runtime_version = runtime_version.ok_or("Failed to fetch runtime version")?;
    let peers = peers.ok_or("Failed to fetch peers")?;
    let sync_state = sync_state.ok_or("Failed to fetch sync state")?;

    let block_data = send_and_receive(&mut socket, "chain_getBlock", json!([block_hash])).await?;

    let metadata = json!({
        "version": version,
        "client": node_name,
        "chain": node_chain,
        "health": node_health,
        "finalized_head": finalized_head,
//        "runtime_version": runtime_version,
        "peers": peers,
        "sync_state": sync_state
    });

    let mut combined_data = block_data.clone();
    combined_data["metadata"] = metadata;

    println!("{}", serde_json::to_string_pretty(&combined_data)?);

    Ok(())
}


async fn get_mmr_proof(endpoint: &str, block_numbers: Option<Vec<u64>>, ipv4: Option<&Ipv4Addr>) -> Result<(), Box<dyn std::error::Error>> {
    let mut socket = if let Some(ip) = ipv4 {
        custom_dns_connect(endpoint, Some(*ip)).await?
    } else {
        let (socket, _) = connect_async(endpoint).await?;
        socket
    };

    let block_numbers = match block_numbers {
        Some(numbers) => numbers,
        None => {
            let head_hash = fetch_block_head_hash(&mut socket).await?;
            let head_number = fetch_block_number(&mut socket, &head_hash).await?;
            vec![head_number]
        }
    };

    let params = json!([block_numbers]);
    let block_data = send_and_receive(&mut socket, "mmr_generateProof", params).await?;

    println!("{}", serde_json::to_string_pretty(&block_data)?);
    Ok(())
}

async fn fetch_block_number(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>, block_hash: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let params = json!([block_hash]);
    let response = send_and_receive(socket, "chain_getBlock", params).await?;
    let block = response.get("block").ok_or("Block key not found in response")?;
    let header = block.get("header").ok_or("Header key not found in response")?;
    let number = header.get("number").ok_or("Number key not found in response")?;
    let block_number_str = number.as_str().ok_or("Block number not found in response")?;
    let block_number = u64::from_str_radix(block_number_str.trim_start_matches("0x"), 16)
                       .map_err(|_| Box::<dyn std::error::Error>::from("Invalid block number format"))?;
    Ok(block_number)
}

async fn fetch_block_head_hash(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> Result<String, Box<dyn std::error::Error>> {
    let params = json!([]);
    let response = send_and_receive(socket, "chain_getHead", params).await?;
    if let Some(hash) = response.as_str() {
        Ok(hash.to_string())
    } else {
        Err("Failed to get block hash as string".into())
    }
}

async fn send_and_receive(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    method: &str,
    params: serde_json::Value
) -> Result<Value, Box<dyn std::error::Error>> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": method,
        "params": params,
    });

    socket.send(Message::Text(request.to_string())).await?;
    // println!("Sent request: {}", request);

    let response = loop {
        let message = socket.next().await.ok_or("Connection closed before receiving response")??;
        if let Message::Text(text) = message {
            let response: Value = serde_json::from_str(&text)?;
            if response["id"] == "1" {
                break response;
            }
        }
    };

    Ok(response["result"].clone())
}
