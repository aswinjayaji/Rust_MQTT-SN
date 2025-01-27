/*
5.4.21 DISCONNECT
Length    MsgType Duration (optional)
(octet 0) (1)     (2-3)
Table 24: DISCONNECT Message
The format of the DISCONNECT message is illustrated in Table 24:
• Length and MsgType: see Section 5.2.
• Duration: contains the value of the sleep timer; this field is optional and is included by a “sleeping” client
that wants to go the “asleep” state, see Section 6.14 for further details.
As with MQTT, the DISCONNECT message is sent by a client to indicate that it wants to close the connection.
The gateway will acknowledge the receipt of that message by returning a DISCONNECT to the client. A server or
gateway may also sends a DISCONNECT to a client, e.g. in case a gateway, due to an error, cannot map a received
message to a client. Upon receiving such a DISCONNECT message, a client should try to setup the connection
again by sending a CONNECT message to the gateway or server. In all these cases the DISCONNECT message
does not contain the Duration field.
A DISCONNECT message with a Duration field is sent by a client when it wants to go to the “asleep” state.
The receipt of this message is also acknowledged by the gateway by means of a DISCONNECT message (without
a duration field).
*/

use bytes::{BufMut, BytesMut};
use custom_debug::Debug;
use getset::{CopyGetters, Getters, MutGetters};
use std::mem;

use crate::{
    broker_lib::MqttSnClient,
    client_id::ClientId,
    connection::Connection,
    connection::StateEnum2,
    eformat,
    filter::get_subscribers_with_topic_id,
    flags::RETAIN_FALSE,
    function,
    keep_alive::KeepAliveTimeWheel,
    msg_hdr::MsgHeader,
    publish::Publish,
    MSG_LEN_DISCONNECT,
    MSG_LEN_DISCONNECT_DURATION,
    // flags::{flags_set, flag_qos_level, },
    MSG_TYPE_DISCONNECT,
};

#[derive(
    Debug,
    Clone,
    Copy,
    Getters,
    /*Setters,*/ MutGetters,
    CopyGetters,
    Default,
)]
#[getset(get, set)]
pub struct Disconnect {
    len: u8,
    #[debug(format = "0x{:x}")]
    msg_type: u8,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Getters,
    /*Setters,*/ MutGetters,
    CopyGetters,
    Default,
)]
#[getset(get, set)]
pub struct DisconnWithDuration {
    len: u8,
    #[debug(format = "0x{:x}")]
    msg_type: u8,
    duration: u16,
}
impl Disconnect {
    pub fn recv(
        buf: &[u8],
        size: usize,
        client: &MqttSnClient,
        msg_header: MsgHeader,
    ) -> Result<(), String> {
        let remote_addr = msg_header.remote_socket_addr;
        if size == MSG_LEN_DISCONNECT as usize {
            let (disconnect, _read_len) =
                Disconnect::try_read(buf, size).unwrap();
            dbg!(disconnect.clone());
            Connection::debug();
            let publish_will;
            match Connection::get_state(&remote_addr) {
                Ok(state) => match state {
                    StateEnum2::ACTIVE => publish_will = true,
                    _ => publish_will = false,
                },
                Err(why) => return Err(eformat!(why, &remote_addr)),
            }
            let conn = Connection::remove(&remote_addr)?;
            ClientId::rev_delete(&remote_addr);
            KeepAliveTimeWheel::cancel(&remote_addr)?;
            Connection::debug();
            Disconnect::send(client, msg_header)?;
            if publish_will == false {
                return Ok(());
            }
            if let Some(topic_id) = conn.will_topic_id {
                let subscriber_vec = get_subscribers_with_topic_id(topic_id);
                for subscriber in subscriber_vec {
                    // Can't return error, because not all subscribers will have error.
                    // TODO error for every subscriber/message
                    // TODO use Bytes not BytesMut to eliminate clone/copy.
                    // TODO new tx method to reduce have try_write() run once for every subscriber.
                    let mut msg = BytesMut::new();
                    msg.put(conn.will_message.clone()); // TODO replace BytesMut with Bytes because clone doesn't copy data in Bytes
                    let _result = Publish::send(
                        topic_id,
                        0, // TODO what is the msg_id?
                        subscriber.qos,
                        RETAIN_FALSE,
                        msg,
                        client,
                        subscriber.socket_addr,
                    );
                }
            }
            Ok(())
        } else if size == MSG_LEN_DISCONNECT_DURATION as usize {
            // *NOTE* Section 6.14 of the MQTT-SN 1.2 spec.
            let (disconnect, _read_len) =
                DisconnWithDuration::try_read(buf, size).unwrap();
            dbg!(disconnect.clone());
            Connection::update_state(&remote_addr, StateEnum2::ASLEEP)?;
            KeepAliveTimeWheel::schedule(remote_addr, disconnect.duration)?;
            Disconnect::send(client, msg_header)?;
            Ok(())
        } else {
            Err(eformat!("len err", size))
        }
    }

    pub fn send(
        client: &MqttSnClient,
        msg_header: MsgHeader,
    ) -> Result<(), String> {
        let disconnect = Disconnect {
            len: MSG_LEN_DISCONNECT as u8,
            msg_type: MSG_TYPE_DISCONNECT,
        };
        let remote_addr = msg_header.remote_socket_addr;
        let mut bytes_buf =
            BytesMut::with_capacity(MSG_LEN_DISCONNECT as usize);
        dbg!(disconnect.clone());
        disconnect.try_write(&mut bytes_buf);
        dbg!(bytes_buf.clone());
        dbg!(remote_addr);
        // transmit to network
        match client
            .egress_tx
            .try_send((remote_addr, bytes_buf.to_owned()))
        {
            Ok(()) => Ok(()),
            Err(err) => Err(eformat!(remote_addr, err)),
        }
    }
}
