use bytes::*;
use core::fmt::Debug;
use crossbeam::channel::*;
use log::*;
use std::net::SocketAddr;
use std::net::UdpSocket;
use std::sync::Arc;
use std::thread;
use util::conn::*;
use webrtc_dtls::config::ExtendedMasterSecretType;
use webrtc_dtls::Error;
use webrtc_dtls::{config::Config, crypto::Certificate, listener::listen};

use crate::{
    advertise::*,
    // Channels::Channels,
    conn_ack::ConnAck,
    connect::Connect,
    connection::Connection,
    dbg_buf,
    disconnect::Disconnect,
    eformat,
    function,
    gw_info::GwInfo,
    hub::Hub,
    keep_alive::KeepAliveTimeWheel,
    msg_hdr::MsgHeader,
    ping_req::PingReq,
    // Connection::ConnHashMap,
    pub_ack::PubAck,
    pub_rel::PubRel,
    publish::Publish,
    retransmit::RetransTimeWheel,
    // search_gw::SearchGw,
    sub_ack::SubAck,
    subscribe::Subscribe,
    will_msg::WillMsg,
    will_topic::WillTopic,
    MSG_TYPE_CONNACK,
    MSG_TYPE_CONNECT,
    MSG_TYPE_DISCONNECT,
    MSG_TYPE_PINGREQ,
    MSG_TYPE_PUBACK,
    MSG_TYPE_PUBLISH,
    MSG_TYPE_PUBREL,
    MSG_TYPE_SUBACK,
    MSG_TYPE_SUBSCRIBE,
    MSG_TYPE_WILL_MSG,
    MSG_TYPE_WILL_TOPIC,
};
// use trace_var::trace_var;

#[derive(Debug, Clone, PartialEq)]
pub enum MessageTypeEnum {
    Connect(Connect),
    ConnAct(ConnAck),
    Subscribe(Subscribe),
    SubAck(SubAck),
    Publish(Publish),
    PubAck(PubAck),
}
pub type IngressChannelType = (SocketAddr, Bytes, Arc<dyn Conn + Send + Sync>);

#[derive(Clone)]
pub struct MqttSnClient {
    // socket: UdpSocket, // clone not implemented
    // state: AtomicU8, // clone not implemented
    pub remote_addr: SocketAddr,
    //    pub local_addr: SocketAddr,
    pub context: u16,

    pub transmit_tx: Sender<(SocketAddr, BytesMut)>,
    // Channel for subscriber to receive messages from the broker
    // publish messages.
    pub subscribe_tx: Sender<Publish>,

    transmit_rx: Receiver<(SocketAddr, BytesMut)>,
    pub subscribe_rx: Receiver<Publish>,
    pub ingress_tx: Sender<IngressChannelType>,
    pub ingress_rx: Receiver<IngressChannelType>,
    pub hub: Arc<Hub>,
    // state: Arc<Mutex<u8>>,
}

impl MqttSnClient {
    // TODO change Client to Broker
    // TODO change remote_addr to local_addr
    pub fn new(remote_addr: SocketAddr) -> Self {
        let (transmit_tx, transmit_rx): (
            Sender<(SocketAddr, BytesMut)>,
            Receiver<(SocketAddr, BytesMut)>,
        ) = unbounded();
        let (subscribe_tx, subscribe_rx): (Sender<Publish>, Receiver<Publish>) =
            unbounded();
        let (ingress_tx, ingress_rx): (
            Sender<IngressChannelType>,
            Receiver<IngressChannelType>,
        ) = unbounded();
        let hub = Arc::new(Hub::new(Arc::new(ingress_tx.clone())));
        MqttSnClient {
            remote_addr,
            context: 0,
            // state: Arc::new(Mutex::new(STATE_DISCONNECT)),
            // keep_alive_time_wheel: KeepAliveTimeWheel::new(),
            transmit_tx,
            transmit_rx,
            subscribe_tx,
            subscribe_rx,
            ingress_tx,
            ingress_rx,
            hub,
            // conn_hashmap: ConnHashMap::new(),
        }
    }

    pub fn handle_ingress(self) {
        let h3 = Arc::clone(&self.hub);
        tokio::spawn(async move {
                        println!("1000 Empty");
            loop {
                match self.ingress_rx.try_recv() {
                    Ok((addr, data, conn)) => {
                        println!("*******************{:?} {:?}", addr, data);
                        
                        // listener.send(addr, data).await?;
                        if let Err(why) = conn.send("hello".as_bytes()).await {
                            println!("Error sending: {:?}", why);
                        }
                        println!("*******************{:?} {:?}", addr, data);
                        /*
                        let conn2 = h3.get_conn(addr).await.unwrap();
                        let _result = conn2
                            .send(
                                "hello there************************"
                                    .as_bytes(),
                            )
                            .await;
                            */
                    }
                    Err(TryRecvError::Empty) => {
                        continue;
                    }
                    Err(why) => {
                        println!("{:?}", why);
                        break;
                    }
                }
            }
        }); 
    }

    pub fn broker_rx_loop(mut self, socket: UdpSocket) {
        let self_transmit = self.clone();
        // name for easy debug
        let socket_tx = socket.try_clone().expect("couldn't clone the socket");
        let builder = thread::Builder::new().name("recv_thread".into());

        let broadcast_socket_addr =
            "224.0.0.123:61000".parse::<SocketAddr>().unwrap();
        let gateway_info_socket_addr =
            "224.0.0.123:62000".parse::<SocketAddr>().unwrap();

        KeepAliveTimeWheel::init();
        KeepAliveTimeWheel::run(self.clone());
        RetransTimeWheel::init();
        RetransTimeWheel::run(self.clone());
        Advertise::run(broadcast_socket_addr, 5, 2);
        GwInfo::run(gateway_info_socket_addr);

        // client runs this to search for gateway.
        // SearchGw::run(gateway_info_socket_addr, 2, 2);

        // process input datagram from network
        let _recv_thread = builder.spawn(move || {
            // TODO optimization
            // recv_from(&mut buf[2..], size -2 ) instead of recv_from(&mut buf size).
            // declare the struct with one:u8 and len:u16
            // if the message header is short, backup 2 bytes to try_read() and len += 2.
            // the message is mapped to the struct with one=0 and correct len.
            // The buf[0..1] must be init to 0.

            let mut buf = [0; 1500];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((size, addr)) => {
                        self.remote_addr = addr;
                        let _result = KeepAliveTimeWheel::reschedule(addr);
                        // Decode message header
                        let msg_header = match MsgHeader::try_read(&buf, size) {
                            Ok(header) => header,
                            Err(e) => {
                                error!("{}", e);
                                continue;
                            }
                        };
                        let msg_type = msg_header.msg_type;
                        if Connection::contains_key(addr) {
                            // Existing connection
                            dbg!(&msg_header);
                            dbg_buf!(buf, size);
                            if msg_type == MSG_TYPE_PUBLISH {
                                if let Err(err) =
                                    Publish::recv(&buf, size, &self, msg_header)
                                {
                                    error!("{}", err);
                                }
                                continue;
                            };
                            if msg_type == MSG_TYPE_PUBREL {
                                if let Err(err) =
                                    PubRel::recv(&buf, size, &self)
                                {
                                    error!("{}", err);
                                }
                                continue;
                            };
                            if msg_type == MSG_TYPE_PUBACK {
                                let _result = PubAck::recv(&buf, size, &self);
                                continue;
                            };
                            if msg_type == MSG_TYPE_PINGREQ {
                                if let Err(err) =
                                    PingReq::recv(&buf, size, &self, msg_header)
                                {
                                    error!("{}", err);
                                }
                                continue;
                            }
                            if msg_type == MSG_TYPE_SUBACK {
                                let _result = SubAck::recv(&buf, size, &self);
                                continue;
                            };
                            if msg_type == MSG_TYPE_SUBSCRIBE {
                                let _result = Subscribe::recv(
                                    &buf, size, &self, msg_header,
                                );
                                continue;
                            };
                            if msg_type == MSG_TYPE_DISCONNECT {
                                let _result =
                                    Disconnect::recv(&buf, size, &mut self);
                                continue;
                            };
                            if msg_type == MSG_TYPE_WILL_TOPIC {
                                if let Err(why) = WillTopic::recv(&buf, size, &self) {
                                    error!("{}", why);
                                }
                                continue;
                            }
                            if msg_type == MSG_TYPE_WILL_MSG {
                                if let Err(why) = WillMsg::recv(&buf, size, &self) {
                                    error!("{}", why);
                                }
                                continue;
                            }
                            if msg_type == MSG_TYPE_CONNACK {
                                match ConnAck::recv(&buf, size, &self) {
                                    // Broker shouldn't receive ConnAck
                                    // because it doesn't send Connect for now.
                                    Ok(_) => {
                                        error!("Broker shouldn't receive ConnAck {:?}", addr);
                                    }
                                    Err(why) => error!("ConnAck {:?}", why),
                                }
                                continue;
                            };
                            error!( "{}", eformat!( addr, "message type not supported:", msg_type));
                        } else {
                            // New connection, not in the connection hashmap.
                            if msg_type == MSG_TYPE_CONNECT {
                                if let Err(err) = Connect::recv(
                                    &buf, size, &mut self, msg_header,
                                ) {
                                    error!("{}", err);
                                }
                                //let clone_socket = socket.try_clone().expect("couldn't clone the socket");
                                // clone_socket.connect(addr).unwrap();
                                continue;
                            }
                            if msg_type == MSG_TYPE_PUBLISH {
                                if let Err(err) = Publish::recv(
                                    &buf, size, &mut self, msg_header,
                                ) {
                                    error!("{}", err);
                                }
                                continue;
                            } else {
                                error!(
                                    "{}",
                                    eformat!(
                                        addr,
                                        "Not in connection map",
                                        msg_type
                                    )
                                );
                                continue;
                            }
                        }
                    }
                    Err(why) => {
                        error!("{}", why);
                    }
                }
            }
        });
        let builder = thread::Builder::new().name("transmit_rx_thread".into());
        // process input datagram from network
        let _transmit_rx_thread = builder.spawn(move || loop {
            match self_transmit.transmit_rx.recv() {
                Ok((addr, bytes)) => {
                    // TODO DTLS
                    dbg!((addr, &bytes));
                    match socket_tx.send_to(&bytes[..], addr) {
                        Ok(size) if size == bytes.len() => (),
                        Ok(size) => {
                            error!(
                                "send_to: {} bytes sent, but {} bytes expected",
                                size,
                                bytes.len()
                            );
                        }
                        Err(why) => {
                            error!("{}", why);
                        }
                    }
                }
                Err(why) => {
                    println!("channel_rx_thread: {}", why);
                }
            }
        });
    }

    /// Connect to a remote broker
    /// 1. send connect message
    /// 2. schedule retransmit
    /// 3. wait for CONNACK
    ///    3.1. receive CONNACK message
    ///    3.2. change state
    /// 4. run the rx_loop to process rx messages
    // TODO return errors
    /*
    pub fn connect(mut self, flags: u8, client_id: String, socket: UdpSocket) {
        let self_time_wheel = self.clone();
        let self_transmit = self.clone();
        let socket_tx = socket.try_clone().expect("couldn't clone the socket");
        let builder = thread::Builder::new().name("send_thread".into());
        // process input datagram from network
        let _send_thread = builder.spawn(move || loop {
            match self_transmit.transmit_rx.recv() {
                Ok((addr, bytes)) => {
                    // TODO DTLS
                    dbg!(("#####", addr, &bytes));
                    let _result = socket_tx.send_to(&bytes[..], addr);
                }
                Err(why) => {
                    println!("channel_rx_thread: {}", why);
                }
            }
        });
        dbg!(&client_id);
        let duration = 5;
        let client_id = Bytes::from(client_id);
        let _result = Connect::send(flags, 1, duration, client_id, &self);
        dbg!(*self.state.lock().unwrap());
        let cur_state = *self.state.lock().unwrap();
        *self.state.lock().unwrap() = self
            .state_machine
            .transition(cur_state, MSG_TYPE_CONNECT)
            .unwrap();
        dbg!(*self.state.lock().unwrap());
        'outer: loop {
            let mut buf = [0; 1500];
            match socket.recv_from(&mut buf) {
                Ok((size, addr)) => {
                    dbg!((size, addr, buf));
                    self.remote_addr = addr;
                    // TODO process 3 bytes length
                    let msg_type = buf[1] as u8;
                    if msg_type == MSG_TYPE_CONNACK {
                        match ConnAck::recv(&buf, size, &self) {
                            Ok(_) => {
                                dbg!(*self.state.lock().unwrap());
                                let cur_state = *self.state.lock().unwrap();
                                *self.state.lock().unwrap() = self
                                    .state_machine
                                    .transition(cur_state, MSG_TYPE_CONNACK)
                                    .unwrap();
                                dbg!(*self.state.lock().unwrap());
                            }
                            Err(why) => error!("ConnAck {:?}", why),
                        }
                        break 'outer;
                    };
                }
                Err(why) => {
                    error!("{}", why);
                }
            }
        }
    }
        */

    pub fn subscribe(
        &self,
        topic: String,
        msg_id: u16,
        qos: u8,
        retain: u8,
    ) -> &Receiver<Publish> {
        let _result = Subscribe::send(topic, msg_id, qos, retain, self);
        &self.subscribe_rx
    }
    /* XXX TODO client code.
    pub fn publish(
        &self,
        topic_id: u16,
        msg_id: u16,
        qos: u8,
        retain: u8,
        data: String,
    ) {
        let _result = Publish::send(topic_id, msg_id, qos, retain, data, &self);
    }
    */
}
