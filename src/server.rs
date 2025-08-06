use prost::Message as _;
use crate::messages::{MonotonicClock,WebSocketClient,SocketCanRx};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use log::{info, error};
use bytes::Bytes;
use std::net::SocketAddr;

use socketcan::{
    CanFrame,EmbeddedFrame, StandardId,
};
use socketcan::tokio::CanSocket;
use futures_util::{
    stream::{SplitSink, SplitStream}};
use socketcan::CanDataFrame;

use socketcan::Frame;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

use tokio::time::{interval, Duration};
use tokio::task::JoinHandle;

use std::sync::{Arc, Mutex};

use lazy_static::lazy_static;



use crate::START_TIME;

// static task handle
lazy_static! {
    static ref TASK_HANDLE1: Arc<Mutex<Option<JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    static ref TASK_HANDLE2: Arc<Mutex<Option<JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    static ref TASK_HANDLE3: Arc<Mutex<Option<JoinHandle<()>>>> = Arc::new(Mutex::new(None));
}

pub async fn run_server(addr: &str,bus:String) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    info!("WebSocket server listening on: {}", addr);


    while let Ok((stream, peer_addr)) = listener.accept().await {
        let bus = bus.clone();

        if let Some(handle) = TASK_HANDLE1.lock().unwrap().take() {
            handle.abort();
        }

        if let Some(handle) = TASK_HANDLE2.lock().unwrap().take() {
            handle.abort();
        }

        if let Some(handle) = TASK_HANDLE3.lock().unwrap().take() {
            handle.abort();
        }

        let handle1 = tokio::spawn(handle_connection(stream, peer_addr,bus));

        *TASK_HANDLE1.lock().unwrap() = Some(handle1);
    }

    Ok(())
}

async fn handle_connection(stream: TcpStream, addr: SocketAddr, bus_name: String) {
    info!("New connection from: {}", addr);
    
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("WebSocket handshake failed");
    
    let (mut write, mut read) = ws_stream.split();

    let socket_can = CanSocket::open(&bus_name);

    match socket_can {
        Ok(_) => {
           
            // 创建通道
            let (torque_tx, mut torque_rx) = mpsc::channel::<f32>(1);
            let (command_tx,mut command_rx)=mpsc::channel::<bool>(1);

            let (motor_tx,mut motor_rx) = mpsc::channel::<String>(10);

            // 启动持续发送任务
            let handle2 = tokio::spawn(async move{
                let bus_name = bus_name.clone();
                
                    let mut current_torque = 0.0f32;
                    let socket_can = CanSocket::open(&bus_name).expect("Failed to open CAN socket");
                    let (mut can_sink,mut can_stream )= socket_can.split(); // 只获取sink部分
                    
                    let mut can_tx = interval(Duration::from_millis(50)); // 10ms发送间隔
                    let mut can_rx = interval(Duration::from_millis(100)); // 100ms接收间隔
                    can_tx.tick().await;
                    can_rx.tick().await;
                    let mut motor_enabled = true;

                    loop {

                        tokio::select! {

                            cmd = command_rx.recv() => {
                                if let Some(enabled) = cmd {
                                    motor_enabled = enabled;
                                    info!("Motor state: {}", enabled);
                                    
                                    // 立即发送状态命令
                                    if enabled {
                                        start_motor(&mut can_sink).await.unwrap_or_else(|e| error!("Start error: {e}"));
                                    } else {
                                        stop_motor(&mut can_sink).await.unwrap_or_else(|e| error!("Stop error: {e}"));
                                    }
                                } else {}

                            }
                            
                            // 扭矩更新（仅在使能时处理）
                            new_torque = torque_rx.recv() => {
                                if motor_enabled{
                                if let Some(torque) = new_torque {
                                    current_torque = torque;
                                    info!("torque updated: {}", torque);
                                } else {}}
                            }                      
                                       
                            // 定时发送 (仅在使能时处理)
                            _ = can_tx.tick() => {
                                if motor_enabled{
                                if let Err(e) = send_can_frame(&mut can_sink,current_torque).await {
                                    error!("Send error: {e}"); break;
                                }}
                            }

                            // 定时接收 (仅在使能时处理)
                            _ = can_rx.tick() => {
                                if motor_enabled{
                                if let Err(e) = handle_can_frame(&mut can_stream,&motor_tx).await {
                                    error!("Recv error: {e}"); break;
                                }}
                            }

                        }
                        }

                });

                *TASK_HANDLE2.lock().unwrap() = Some(handle2);

                let handle3 = tokio::spawn(async move{
                loop{
                    let motor_data = motor_rx.recv().await;
                    match motor_data{
                    Some(data) => {
                    let fb_motor = SocketCanRx {
                        motor_data: data,
                        receive_time: Some(MonotonicClock {
                        us_since_system_boot: START_TIME.elapsed().as_micros() as u32,}),
                    };
                    if let Err(_) = write.send(Message::Binary(fb_motor.encode_to_vec())).await {}}
                        None => {}}}});
                *TASK_HANDLE3.lock().unwrap() = Some(handle3); 
                               

            // 消息处理循环
            while let Some(Ok(msg)) = read.next().await {
                match msg {
                    Message::Binary(data) => {
                        match WebSocketClient::decode(Bytes::from(data)) {
                            Ok(message) => {
                                info!("Received command: {}, data: {}", message.command, message.data);
                                
                                // 更新扭矩值
                                let torque = message.data;
                                let command =message.command;
                                    // 发送新值到通道
                                if let Err(e) = command_tx.send(command).await {
                                    error!("Failed to send command to CAN task: {}", e);
                                    break;
                                }
                                if let Err(e) = torque_tx.send(torque).await {
                                    error!("Failed to send torque to CAN task: {}", e);
                                    break;
                                }

                            }
                            Err(e) => error!("Decoding error: {}", e),
                        }
                    }
                    Message::Close(_) => {
                        info!("Client disconnected: {}", addr);
                        break;
                    }
                    _ => info!("Received non-binary message"),
                }
            }

        }
        Err(e) => {
            error!("CAN open error: {}", e);
        }
    }

}

fn float_to_uint(x:f32,x_min:f32,x_max:f32,bits:u8)->i16
{
    let span =x_max -x_min;
    let offset =x_min;

    ((x-offset)*(((1<<bits)-1)as f32)/span)as i16
}

fn unit_to_float(x:u16,x_min:f32,x_max:f32,bits:u8)->f32
{
    let span = x_max - x_min;
    let offset = x_min;

    (x as f32)*span/(((1<<bits)-1) as f32)+offset
}

async fn stop_motor(
    can_sink: & mut SplitSink<CanSocket, CanFrame>
) -> Result<(), Box<dyn std::error::Error>> {

    let frame = CanDataFrame::new(
        StandardId::new(0x10).unwrap(),
        &[
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFD,
        ],
    )
    .unwrap();

   can_sink.send(socketcan::CanFrame::Data(frame)).await?;
   Ok(())
}

// 启动电机
async fn start_motor(
    can_sink: & mut SplitSink<CanSocket, CanFrame>
) -> Result<(), Box<dyn std::error::Error>> {
    
    let frame = CanDataFrame::new(
        StandardId::new(0x10).unwrap(),
        &[
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFC,
        ],
    )
    .unwrap();

   can_sink.send(socketcan::CanFrame::Data(frame)).await?;
   Ok(())
}

// 发送CAN帧
async fn send_can_frame(
    can_sink: & mut SplitSink<CanSocket, CanFrame>,
    torque: f32
) -> Result<(), Box<dyn std::error::Error>> {

    tokio::time::sleep(Duration::from_millis(10)).await;
    let torque_uint = float_to_uint(torque,-10.0,10.0,12);

    let frame = CanDataFrame::new(
        StandardId::new(0x10).unwrap(),
        &[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ((torque_uint >> 8) & 0xFF) as u8,
            (torque_uint & 0xFF) as u8,
        ],
    )
    .unwrap();
    //info!("motor run");
    can_sink.send(socketcan::CanFrame::Data(frame)).await?;
    Ok(())
}

//接收CAN反馈
async fn handle_can_frame(
    can_stream: & mut SplitStream<CanSocket>,
    motor_tx: &Sender<String>
) -> Result<(), Box<dyn std::error::Error>> {

     tokio::time::sleep(Duration::from_millis(10)).await;
    if let Some(Ok(socketcan::CanFrame::Data(frame))) = can_stream.next().await {
        let mask = 0x01;
            if frame.raw_id() & 0xFF == mask {
                let can_frame=frame.data();
                let pos_bytes =((can_frame[1] as u16)<<8)|(can_frame[2] as u16);
                let spd_bytes=((can_frame[3] as u16)<<4)|((can_frame[4]>>4)as u16);
                let t_bytes =(((can_frame[4]&0x0F)as u16)<<8)|(can_frame[5] as u16);

                let pos= unit_to_float(pos_bytes,-3.141593,3.141593,16);
                let spd= unit_to_float(spd_bytes,-50.0,50.0,12);
                let t= unit_to_float(t_bytes,-10.0,10.0,12);

                let motor_data = format!(
                    "POS:{},SPD:{},T:{}",pos,spd,t
                );

                if let Err(e) = motor_tx.send(motor_data).await {
                    error!("Failed to send motor_data: {}", e);
                }

            }
    }
    Ok(())
}
