use prost::Message as _;
use crate::messages::{MonotonicClock,WebSocketClient,SocketCanRx};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use clap::Parser;
use log::info;
use bytes::Bytes;

use crate::START_TIME;

#[derive(Parser, Debug)]
#[command(version, about)]
pub struct ClientArgs {
    #[arg(short, long, default_value = "ws://127.0.0.1:8080")]
    url: String,
    
    #[arg(short, long)]
    command: bool,
    
    #[arg(short, long, default_value = "")]
    data: f32,
}


pub async fn run_client(args: ClientArgs) -> Result<(), Box<dyn std::error::Error>> {
    info!("Connecting to: {}", args.url);
    let (ws_stream, _) = connect_async(&args.url).await?;
    let (mut write, mut read) = ws_stream.split();
    
    // 创建 protobuf 消息
    let message = WebSocketClient {
        command: args.command,
        data: args.data,
        receive_time: Some(MonotonicClock {
            us_since_system_boot: START_TIME.elapsed().as_micros() as u32,
            })
    };
    
    // 发送消息
    write.send(Message::Binary(message.encode_to_vec())).await?;
    
    if args.command{

            // 等待响应
    while let Some(Ok(msg)) = read.next().await {
        
        if let Message::Binary(data) = msg {
            match SocketCanRx::decode(Bytes::from(data)) {
                Ok(response) => {
                    println!(
                        "Server response: {}\nreceive_time: {}",
                        response.motor_data,response.receive_time.expect("Missing MonotonicClock").us_since_system_boot
                    );
                    
                }
                Err(e) => eprintln!("Decoding error: {}", e),
            }
        }
    }
    }else{
        info!("Motor stop...")
    }
    
    Ok(())
}