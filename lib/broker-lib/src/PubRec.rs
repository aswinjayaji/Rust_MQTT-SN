use bytes::{BufMut, BytesMut};
use custom_debug::Debug;
use getset::{CopyGetters, Getters, MutGetters, Setters};
use std::mem;

use crate::{
    BrokerLib::MqttSnClient,
    Errors::ExoError,
    // flags::{flags_set, flag_qos_level, },
    MSG_LEN_PUBREC,
    MSG_TYPE_PUBREC,
};
#[derive(
    Debug, Clone, Getters, Setters, MutGetters, CopyGetters, Default, PartialEq,
)]
#[getset(get, set)]
pub struct PubRec {
    pub len: u8,
    #[debug(format = "0x{:x}")]
    pub msg_type: u8,
    pub msg_id: u16,
}

impl PubRec {
    fn constraint_len(_val: &u8) -> bool {
        //dbg!(_val);
        true
    }
    fn constraint_msg_type(_val: &u8) -> bool {
        //dbg!(_val);
        true
    }
    fn constraint_msg_id(_val: &u16) -> bool {
        //dbg!(_val);
        true
    }
    #[inline(always)]
    pub fn rx(
        buf: &[u8],
        size: usize,
        client: &MqttSnClient,
    ) -> Result<u16, ExoError> {
        if buf[0] == MSG_LEN_PUBREC && buf[1] == MSG_TYPE_PUBREC {
            // TODO verify as Big Endian
            let msg_id = buf[2] as u16 + ((buf[3] as u16) << 8);
            let _result = client.cancel_tx.send((
                client.remote_addr,
                MSG_TYPE_PUBREC,
                0,
                msg_id,
            ));
            Ok(msg_id)
        } else {
            Err(ExoError::LenError(buf[0] as usize, MSG_LEN_PUBREC as usize))
        }
    }
    #[inline(always)]
    pub fn tx(msg_id: u16, client: &MqttSnClient) -> BytesMut {
        // faster implementation
        // TODO verify big-endian or little-endian for u16 numbers
        // XXX order of statements performance
        let msg_id_byte_0 = msg_id as u8;
        let msg_id_byte_1 = (msg_id >> 8) as u8;
        // message format
        // PUBACK:[len(0), msg_type(1), msg_id(2,3)]
        let mut bytes = BytesMut::with_capacity(MSG_LEN_PUBREC as usize);
        let buf: &[u8] = &[
            MSG_LEN_PUBREC,
            MSG_TYPE_PUBREC,
            msg_id_byte_1,
            msg_id_byte_0,
        ];
        bytes.put(buf);
        // TODO replace BytesMut with Bytes to eliminate clone as copy
        let _result =
            client.transmit_tx.send((client.remote_addr, bytes.clone()));
        dbg!(&buf);
        bytes
    }
}
