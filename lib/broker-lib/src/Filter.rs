use hashbrown::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bisetmap::BisetMap;

// use crate::Connection::ConnId;
use std::net::SocketAddr;
//use uuid::v1::{Context, Timestamp};
//use uuid::Uuid;

use crate::flags::{
    QoSConst, QOS_LEVEL_0, QOS_LEVEL_1, QOS_LEVEL_2, QOS_LEVEL_3,
};

macro_rules! function {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        &name[..name.len() - 3]
    }};
}

/// Checks if a topic or topic filter has wildcards
#[inline(always)]
pub fn has_wildcards(filter: &str) -> bool {
    filter.contains('+') || filter.contains('#')
}

// https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html#_Toc398718106
// A subscription topic filter can contain # or + to allow the client to
// subscribe to multiple topics at once.
#[inline(always)]
pub fn valid_filter(filter: &str) -> bool {
    if !filter.is_empty() {
        if has_wildcards(filter) {
            // Verify multi level wildcards.
            if filter.find('#') == Some(filter.len() - 1)
                && filter.ends_with("/#")
            {
                return true;
            }
        // TODO verify single level wildcards.
        } else {
            return true;
        }
    }
    false
}

// XXX copy from rumqtt
/// Checks if topic matches a filter. topic and filter validation isn't done here.
///
/// **NOTE**: 'topic' is a misnomer in the arg. this can also be used to match 2 wild subscriptions
/// **NOTE**: make sure a topic is validated during a publish and filter is validated
/// during a subscribe
#[inline(always)]
pub fn match_topic(topic: &str, filter: &str) -> bool {
    if !topic.is_empty() && topic[..1].contains('$') {
        return false;
    }

    let mut topics = topic.split('/');
    let mut filters = filter.split('/');

    for f in filters.by_ref() {
        // "#" being the last element is validated by the broker with 'valid_filter'
        if f == "#" {
            return true;
        }

        // filter still has remaining elements
        // filter = a/b/c/# should match topci = a/b/c
        // filter = a/b/c/d should not match topic = a/b/c
        let top = topics.next();
        match top {
            Some(t) if t == "#" => return false,
            Some(_) if f == "+" => continue,
            Some(t) if f != t => return false,
            Some(_) => continue,
            None => return false,
        }
    }

    // topic has remaining elements and filter's last element isn't "#"
    if topics.next().is_some() {
        return false;
    }

    true
}

#[derive(Debug, Clone)]
pub struct Filter {
    wildcard_topics: HashMap<String, Arc<Mutex<HashSet<SocketAddr>>>>,
    wildcard_filters: HashMap<String, Arc<Mutex<HashSet<SocketAddr>>>>,
    concrete_topics: HashMap<String, Arc<Mutex<HashSet<SocketAddr>>>>,
    id_topics: HashMap<u16, Arc<Mutex<HashSet<SocketAddr>>>>, // only MQTT-SN
}

#[derive(Debug, Clone)]
pub struct Filter2 {
    // wildcard_topics: HashMap<String, Arc<Mutex<HashSet<SocketAddr>>>>,
    // wildcard_filters: HashMap<String, Arc<Mutex<HashSet<SocketAddr>>>>,
    concrete_topics: BisetMap<String, SocketAddr>,
    // id_topics: HashMap<u16, Arc<Mutex<HashSet<SocketAddr>>>>, // only MQTT-SN
}

impl Filter2 {
    pub fn new() -> Self {
        Filter2 {
            // wildcard_topics: HashMap::new(),
            // wildcard_filters: HashMap::new(),
            concrete_topics: BisetMap::new(),
            // id_topics: HashMap::new(), // only MQTT-SN
        }
    }
    pub fn insert_topic(&mut self, topic: &str, addr: SocketAddr) {
        self.concrete_topics.insert(topic.to_string(), addr);
    }
}

impl Filter {
    pub fn new() -> Self {
        Filter {
            wildcard_topics: HashMap::new(),
            wildcard_filters: HashMap::new(),
            concrete_topics: HashMap::new(),
            id_topics: HashMap::new(), // only MQTT-SN
        }
    }
    /// only MQTT-SN
    // TODO write tests for this
    pub fn insert_id_topic(
        &mut self,
        id: u16,
        socket_addr: SocketAddr,
    ) -> Result<(), String> {
        let conn_set = self
            .id_topics
            .entry(id)
            .or_insert(Arc::new(Mutex::new(HashSet::new())));
        let mut conn_set = conn_set.lock().unwrap();
        if conn_set.insert(socket_addr) {
            return Ok(());
        } else {
            // duplicate entry
            return Err(format!(
                "{}: {} already subscribed to {}",
                function!(),
                socket_addr,
                id
            ));
        }
    }
    /// Insert a new filter/subscription string from a connection subscription.
    #[inline(always)]
    pub fn insert(
        &mut self,
        filter: &str,
        socket_addr: SocketAddr,
    ) -> Result<(), String> {
        if valid_filter(filter) {
            if has_wildcards(filter) {
                let conn_set = self
                    .wildcard_filters
                    .entry(filter.to_string())
                    .or_insert(Arc::new(Mutex::new(HashSet::new())));
                let mut conn_set = conn_set.lock().unwrap();
                if conn_set.insert(socket_addr) {
                    return Ok(());
                } else {
                    // duplicate entry
                    return Err(format!(
                        "{}: {} already subscribed to {}",
                        function!(),
                        socket_addr,
                        filter
                    ));
                }
            } else {
                let conn_set = self
                    .concrete_topics
                    .entry(filter.to_string())
                    .or_insert(Arc::new(Mutex::new(HashSet::new())));
                let mut conn_set = conn_set.lock().unwrap();
                if conn_set.insert(socket_addr) {
                    return Ok(());
                } else {
                    // duplicate entry
                    return Err(format!(
                        "{}: {} already subscribed to {}",
                        function!(),
                        socket_addr,
                        filter
                    ));
                }
            }
        }
        Err(format!("{}: invalid filter: {}.", function!(), filter))
    }

    #[inline(always)]
    pub fn match_topic_id(
        &mut self,
        topic: u16,
    ) -> Option<HashSet<SocketAddr>> {
        if let Some(id_set) = self.id_topics.get(&topic) {
            return Some(id_set.lock().unwrap().clone());
        }
        None
    }

    #[inline(always)]
    pub fn match_topic_concrete(
        &mut self,
        topic: &str,
    ) -> Option<HashSet<SocketAddr>> {
        if let Some(id_set) = self.concrete_topics.get(topic) {
            return Some(id_set.lock().unwrap().clone());
        }
        None
    }

    #[inline(always)]
    pub fn match_topic_wildcard(
        &mut self,
        topic: &str,
    ) -> Option<HashSet<SocketAddr>> {
        // Topic is in the wildcard_topics map.
        if let Some(id_set) = self.wildcard_topics.get(topic) {
            return Some(id_set.lock().unwrap().clone());
        } else {
            // Publish topic shouldn't have wildcards.
            if has_wildcards(topic) {
                return None;
            }
            // Match the topic against all wildcard filters.
            // Insert the topic into wildcard_topics if matched.
            for (filter, id_set) in &self.wildcard_filters {
                // dbg!((filter, id_set));
                if match_topic(topic, filter) {
                    // dbg!((filter, id_set));
                    self.wildcard_topics
                        .insert(topic.to_string(), id_set.clone());
                }
            }
            // Return the topic's wildcard_topics set.
            if let Some(id_set) = self.wildcard_topics.get(topic) {
                return Some(id_set.lock().unwrap().clone());
            }
        }
        None
    }

    // Doesn't work correctly.
    pub fn match_topic(&mut self, topic: &str) -> Option<HashSet<SocketAddr>> {
        // Publish topic shouldn't have wildcards.
        if has_wildcards(topic) {
            return None;
        }

        let mut new_set: HashSet<SocketAddr> = HashSet::new();
        if let Some(socket_set) = self.wildcard_topics.get(topic) {
            // return Some(socket_set.lock().unwrap().clone());
            let wildcard_set = socket_set.lock().unwrap().clone();
            new_set.extend(&wildcard_set);
        } else {
            for (filter, socket_set) in &self.wildcard_filters {
                dbg!((filter, socket_set));
                if match_topic(topic, filter) {
                    dbg!((filter, socket_set));
                    self.wildcard_topics
                        .insert(topic.to_string(), socket_set.clone());
                }
            }
        }
        if let Some(socket_set) = self.concrete_topics.get(topic) {
            // return Some(socket_set.lock().unwrap().clone());
            let concrete_set = socket_set.lock().unwrap().clone();
            new_set.extend(&concrete_set);
        }
        if !new_set.is_empty() {
            return Some(new_set);
        }
        None
    }
}

pub type TopicIdType = u16;

lazy_static! {
    pub static ref GLOBAL_FILTERS: Mutex<Filter> = Mutex::new(Filter::new());
    pub static ref GLOBAL_CONCRETE_TOPICS: Mutex<BisetMap<String, SocketAddr>> =
        Mutex::new(BisetMap::new());
    pub static ref GLOBAL_WILDCARD_TOPICS: Mutex<BisetMap<String, SocketAddr>> =
        Mutex::new(BisetMap::new());
    pub static ref GLOBAL_WILDCARD_FILTERS: Mutex<BisetMap<String, SocketAddr>> =
        Mutex::new(BisetMap::new());
    pub static ref GLOBAL_TOPIC_IDS: Mutex<BisetMap<TopicIdType, SocketAddr>> =
        Mutex::new(BisetMap::new());
    pub static ref GLOBAL_TOPIC_IDS_QOS: Mutex<BisetMap<(TopicIdType, SocketAddr), QoSConst>> =
        Mutex::new(BisetMap::new());
    /// Topic name to topic id map is 1:1. Using a BisetMap to allow access from both sides.
    pub static ref GLOBAL_TOPIC_NAME_TO_IDS: Mutex<BisetMap<String, TopicIdType>> =
        Mutex::new(BisetMap::new());
    pub static ref GLOBAL_TOPIC_ID: Mutex<TopicIdType> = Mutex::new(0);
}

pub fn try_insert_topic_name(
    topic_name: String,
) -> Result<TopicIdType, String> {
    let topic_ids = GLOBAL_TOPIC_NAME_TO_IDS.lock().unwrap().get(&topic_name);
    // If topic name is already in the map, return the existing topic id,
    // otherwise insert the topic name and topic id into the map.
    if topic_ids.len() == 0 {
        let topic_id = *GLOBAL_TOPIC_ID.lock().unwrap();
        GLOBAL_TOPIC_NAME_TO_IDS
            .lock()
            .unwrap()
            .insert(topic_name, topic_id);
        *GLOBAL_TOPIC_ID.lock().unwrap() = topic_id + 1;
        return Ok(topic_id);
    } else {
        // Topic name is already in the map with only one topic id.
        return Ok(topic_ids[0]);
    }
}

#[inline(always)]
pub fn insert_subscriber_with_topic_id(
    socket_add: SocketAddr,
    id: TopicIdType,
    qos: QoSConst,
) -> Result<(), String> {
    GLOBAL_TOPIC_IDS.lock().unwrap().insert(id, socket_add);
    GLOBAL_TOPIC_IDS_QOS
        .lock()
        .unwrap()
        .insert((id, socket_add), qos);
    Ok(())
}

#[inline(always)]
pub fn get_subscribers_with_topic_id(id: u16) -> Vec<(SocketAddr, QoSConst)> {
    let sock_vec = GLOBAL_TOPIC_IDS.lock().unwrap().get(&id);
    let mut return_vec: Vec<(SocketAddr, QoSConst)> = Vec::new();
    for sock in sock_vec {
        let qos_vec = GLOBAL_TOPIC_IDS_QOS.lock().unwrap().get(&(id, sock));
        for qos in qos_vec {
            return_vec.push((sock, qos));
        }
    }
    return_vec
}

#[inline(always)]
pub fn delete_subscribers_with_socket_addr(socket_addr: &SocketAddr) {
    GLOBAL_TOPIC_IDS.lock().unwrap().rev_delete(socket_addr);
}

#[inline(always)]
pub fn insert_filter(
    filter: String,
    socket_add: SocketAddr,
) -> Result<(), String> {
    if valid_filter(&filter[..]) {
        if has_wildcards(&filter[..]) {
            GLOBAL_WILDCARD_FILTERS
                .lock()
                .unwrap()
                .insert(filter, socket_add);
        } else {
            GLOBAL_CONCRETE_TOPICS
                .lock()
                .unwrap()
                .insert(filter.clone(), socket_add);
        }
        return Ok(());
    }
    Err(format!("{}: invalid filter: {}.", function!(), filter))
}

/// Remove topics and filters from the bisetmaps using the rev_delete()
#[inline(always)]
pub fn delete_filter(socket_add: SocketAddr) {
    GLOBAL_WILDCARD_FILTERS
        .lock()
        .unwrap()
        .rev_delete(&socket_add);
    GLOBAL_CONCRETE_TOPICS
        .lock()
        .unwrap()
        .rev_delete(&socket_add);
    GLOBAL_WILDCARD_TOPICS
        .lock()
        .unwrap()
        .rev_delete(&socket_add);
}

#[inline(always)]
pub fn match_concrete_topics(topic: &String) -> Vec<SocketAddr> {
    GLOBAL_CONCRETE_TOPICS.lock().unwrap().get(&topic)
}

#[inline(always)]
pub fn match_topics(topic: &String) -> Vec<SocketAddr> {
    let sock_vec = GLOBAL_WILDCARD_TOPICS.lock().unwrap().get(topic);
    if sock_vec.len() == 0 {
        // The topic doesn't match any wildcard topics.
        // Matching the topic against all wildcard filters.
        for (filter, socket_vec) in
            GLOBAL_WILDCARD_FILTERS.lock().unwrap().collect()
        {
            if match_topic(topic, &filter) {
                // Insert each socket_addr into the matching wildcard_topics.
                for sock in socket_vec {
                    GLOBAL_WILDCARD_TOPICS
                        .lock()
                        .unwrap()
                        .insert(topic.clone(), sock);
                }
            }
        }
    }
    let wildcards = GLOBAL_WILDCARD_TOPICS.lock().unwrap().get(topic);
    let mut concretes = GLOBAL_CONCRETE_TOPICS.lock().unwrap().get(topic);
    concretes.append(&mut wildcards.clone());
    concretes.sort();
    concretes.dedup();
    concretes
}

pub fn global_filter_insert(
    filter: &str,
    socket_add: SocketAddr,
) -> Result<(), String> {
    let mut filters = GLOBAL_FILTERS.lock().unwrap();
    filters.insert(filter, socket_add)?;
    dbg!(filters);
    Ok(())
}

#[cfg(test)]
mod test {
    use uuid::Uuid;

    use uuid::v1::{Context, Timestamp};

    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_topic_name_and_id() {
        let topic_id =
            super::try_insert_topic_name("test".to_string()).unwrap();
        assert_eq!(topic_id, 0);
        let topic_id =
            super::try_insert_topic_name("test".to_string()).unwrap();
        assert_eq!(topic_id, 0);
        let topic_id =
            super::try_insert_topic_name("test/now".to_string()).unwrap();
        assert_eq!(topic_id, 1);
        dbg!(super::GLOBAL_TOPIC_NAME_TO_IDS.lock().unwrap());
        dbg!(super::GLOBAL_TOPIC_ID.lock().unwrap());
    }
    #[test]
    fn test_topic_id() {
        use crate::flags::{
            QoSConst, QOS_LEVEL_0, QOS_LEVEL_1, QOS_LEVEL_2, QOS_LEVEL_3,
        };
        use std::net::SocketAddr;

        let socket = "127.0.0.1:1200".parse::<SocketAddr>().unwrap();
        let socket2 = "127.0.0.2:1200".parse::<SocketAddr>().unwrap();
        let socket3 = "127.0.0.3:1200".parse::<SocketAddr>().unwrap();
        let socket4 = "127.0.0.4:1200".parse::<SocketAddr>().unwrap();
        super::insert_subscriber_with_topic_id(socket, 1, QOS_LEVEL_2);
        super::insert_subscriber_with_topic_id(socket2, 1, QOS_LEVEL_1);
        super::insert_subscriber_with_topic_id(socket3, 1, QOS_LEVEL_0);
        super::insert_subscriber_with_topic_id(socket, 2, QOS_LEVEL_2);
        super::insert_subscriber_with_topic_id(socket2, 2, QOS_LEVEL_1);
        super::insert_subscriber_with_topic_id(socket3, 3, QOS_LEVEL_0);
        dbg!(super::GLOBAL_TOPIC_IDS.lock().unwrap());
        dbg!(super::GLOBAL_TOPIC_IDS_QOS.lock().unwrap());
        let result = super::get_subscribers_with_topic_id(1);
        dbg!(result);
        let result = super::get_subscribers_with_topic_id(2);
        dbg!(result);
        let result = super::get_subscribers_with_topic_id(3);
        dbg!(result);
    }

    #[test]
    fn test_insert_filter() {
        use std::net::SocketAddr;

        let socket = "127.0.0.1:1200".parse::<SocketAddr>().unwrap();
        let socket2 = "127.0.0.2:1200".parse::<SocketAddr>().unwrap();
        let socket3 = "127.0.0.3:1200".parse::<SocketAddr>().unwrap();
        let socket4 = "127.0.0.4:1200".parse::<SocketAddr>().unwrap();
        super::insert_filter("hello".to_string(), socket);
        super::insert_filter("hello".to_string(), socket2);
        super::insert_filter("hello/world".to_string(), socket);
        super::insert_filter("hello/world".to_string(), socket2);
        super::insert_filter("hello/world".to_string(), socket4);
        super::insert_filter("hello/#".to_string(), socket);
        super::insert_filter("hello/#".to_string(), socket2);
        super::insert_filter("hello/world/#".to_string(), socket);
        super::insert_filter("hello/world/#".to_string(), socket2);
        super::insert_filter("hello/world/#".to_string(), socket3);
        dbg!(super::GLOBAL_CONCRETE_TOPICS.lock().unwrap());
        dbg!(super::GLOBAL_WILDCARD_FILTERS.lock().unwrap());
        let result = super::match_topics(&"hello".to_string());
        dbg!(result);
        let result = super::match_topics(&"hello/world".to_string());
        dbg!(result);
        let result = super::match_topics(&"hi".to_string());
        dbg!(result);
        let result = super::match_topics(&"hello/there".to_string());
        dbg!(result);
        let result = super::match_topics(&"hello/world/there".to_string());
        dbg!(result);

        dbg!(super::GLOBAL_CONCRETE_TOPICS.lock().unwrap());
        dbg!(super::GLOBAL_WILDCARD_FILTERS.lock().unwrap());
        dbg!(super::GLOBAL_WILDCARD_TOPICS.lock().unwrap());
        super::delete_filter(socket2);
        dbg!(super::GLOBAL_CONCRETE_TOPICS.lock().unwrap());
        dbg!(super::GLOBAL_WILDCARD_FILTERS.lock().unwrap());
        dbg!(super::GLOBAL_WILDCARD_TOPICS.lock().unwrap());
    }
    #[test]
    fn test_filter2_insert_topic() {
        use std::net::SocketAddr;

        let socket = "127.0.0.1:1200".parse::<SocketAddr>().unwrap();
        let socket2 = "127.0.0.2:1200".parse::<SocketAddr>().unwrap();

        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .insert("/test".to_string(), socket);
        // Duplicate entry, one entry should be inserted.
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .insert("/test".to_string(), socket);
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .insert("/test".to_string(), socket2);
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .insert("/test2".to_string(), socket);
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .insert("/test2".to_string(), socket2);
        dbg!(super::GLOBAL_CONCRETE_TOPICS.lock().unwrap());
        let result = super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .get(&"/test".to_string());
        dbg!(result);
        let result = super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .rev_get(&socket);
        dbg!(result);
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .remove(&"/test".to_string(), &socket);
        dbg!(super::GLOBAL_CONCRETE_TOPICS.lock().unwrap());
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .insert("/test".to_string(), socket);
        dbg!(super::GLOBAL_CONCRETE_TOPICS.lock().unwrap());
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .delete(&"/test".to_string());
        dbg!(super::GLOBAL_CONCRETE_TOPICS.lock().unwrap());
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .insert("/test".to_string(), socket);
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .insert("/test".to_string(), socket2);
        super::GLOBAL_CONCRETE_TOPICS
            .lock()
            .unwrap()
            .rev_delete(&socket2);
        dbg!(super::GLOBAL_CONCRETE_TOPICS.lock().unwrap());

        /*

        let mut filter2 = super::Filter2::new();
        filter2.insert_topic("hello", socket);
        filter2.insert_topic("hello", socket2);
        filter2.insert_topic("hi", socket);
        filter2.insert_topic("hi", socket2);
        dbg!(filter2);
        */
    }
    #[test]
    fn test_insert() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let socket = "127.0.0.1:1200".parse::<SocketAddr>().unwrap();
        let socket_str = socket.to_string();
        dbg!(socket_str);
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        println!("{:?}", since_the_epoch);
        let in_ns = since_the_epoch.as_nanos() as u32;
        let in_s = since_the_epoch.as_secs();
        println!("{:?}", in_ns);
        println!("{:?}", in_s);
        let context = Context::new(42);
        let ts = Timestamp::from_unix(&context, in_s, in_ns);
        let mut ip4bytes: [u8; 4] = [0; 4];
        let port_bytes: [u8; 2] = socket.port().to_be_bytes();

        match socket.ip() {
            IpAddr::V4(ip4) => ip4bytes = ip4.octets(),
            IpAddr::V6(ip6) => {
                println!("ipv6: {}, segments: {:?}", ip6, ip6.segments())
            }
        }
        dbg!(ip4bytes);
        dbg!(port_bytes);
        let zz: [u8; 6] = [
            ip4bytes[0],
            ip4bytes[1],
            ip4bytes[2],
            ip4bytes[3],
            port_bytes[0],
            port_bytes[1],
        ];
        dbg!(zz);

        let uuid = Uuid::new_v1(ts, &zz).expect("failed to generate UUID");
        dbg!((&context, ts, uuid));
        let uuid =
            Uuid::new_v1(ts, b"123456").expect("failed to generate UUID");
        dbg!((&context, ts, uuid));
        let context = Context::new(42);
        let ts = Timestamp::from_unix(&context, in_s, in_ns);
        let uuid = Uuid::new_v1(ts, &[192, 168, 0, 4, 8, 7])
            .expect("failed to generate UUID");
        dbg!((&context, ts, uuid));
        let context = Context::new(45);
        let ts = Timestamp::from_unix(&context, in_s, in_ns);
        let uuid = Uuid::new_v1(ts, &[1, 2, 3, 4, 5, 6])
            .expect("failed to generate UUID");
        dbg!((context, ts, uuid));

        let mut filter = super::Filter::new();
        filter.insert("aa/bb", socket);
        filter.insert("aa/cc", socket);
        filter.insert("aa/bb", socket);
        let mut r = filter.match_topic("aa/bb").unwrap();
        dbg!(&r);
        dbg!(&filter);

        filter.insert("aa/#", socket);
        filter.insert("aa/#", socket);
        filter.insert("bb/+", socket);
        let r = filter.match_topic_concrete("bb/bb");
        dbg!(&r);
        let r = filter.match_topic_concrete("bb/bb/cc");
        dbg!(&r);
        let r = filter.match_topic_concrete("aa/bb");
        dbg!(&r);
        let r = filter.match_topic_wildcard("aa/dd");
        dbg!(&r);
        let r = filter.match_topic_wildcard("aa/ee/ff");
        dbg!(&r);
        let r = filter.match_topic_wildcard("zz/dd");
        dbg!(&r);
        dbg!(&filter);
    }

    /*
    #[test]
    fn filer_add() {
        let mut filter = super::Filter::new();
        assert!(filter.add("a/b/c"));
        assert!(filter.add("a/b/#"));
        dbg!(filter);
    }

    #[test]
    fn filer_match() {
        let mut filter = super::Filter::new();
        assert!(filter.add("a/b/c"));
        assert!(filter.add("a/b/#"));
        // TODO implement + wildcard
        assert!(!filter.add("a/+/e"));
        assert!(!filter.match_topic("a/b/#"));
        assert!(filter.match_topic("a/b/c"));
        assert!(filter.match_topic("a/b/d"));
        assert!(filter.match_topic("a/b/e"));
        dbg!(filter);
    }

    #[test]
    fn wildcards_are_detected_correctly() {
        assert!(!super::has_wildcards("a/b/c"));
        assert!(super::has_wildcards("a/+/c"));
        assert!(super::has_wildcards("a/b/#"));
    }

    #[test]
    fn filters_are_validated_correctly() {
        assert!(!super::valid_filter("wrong/#/filter"));
        assert!(!super::valid_filter("wrong/wr#ng/filter"));
        assert!(!super::valid_filter("wrong/filter#"));
        assert!(super::valid_filter("correct/filter/#"));
        assert!(super::valid_filter("correct/filter/"));
        assert!(super::valid_filter("correct/filter"));
        assert!(!super::valid_filter(""));
    }

    #[test] // TODO learn more about this from rumqtt
    fn dollar_subscriptions_doesnt_match_dollar_topic() {
        assert!(super::match_topic("sy$tem/metrics", "sy$tem/+"));
        assert!(!super::match_topic("$system/metrics", "$system/+"));
        assert!(!super::match_topic("$system/metrics", "+/+"));
    }

    #[test]
    fn topics_match_with_filters_as_expected() {
        let topic = "a/b/c";
        let filter = "a/b/c";
        assert!(super::match_topic(topic, filter));

        let topic = "a/b/c";
        let filter = "d/b/c";
        assert!(!super::match_topic(topic, filter));

        let topic = "a/b/c";
        let filter = "a/b/e";
        assert!(!super::match_topic(topic, filter));

        let topic = "a/b/c";
        let filter = "a/b/c/d";
        assert!(!super::match_topic(topic, filter));

        let topic = "a/b/c";
        let filter = "#";
        assert!(super::match_topic(topic, filter));

        let topic = "a/b/c";
        let filter = "a/b/c/#";
        assert!(super::match_topic(topic, filter));

        let topic = "a/b/c/d";
        let filter = "a/b/c";
        assert!(!super::match_topic(topic, filter));

        let topic = "a/b/c/d";
        let filter = "a/b/c/#";
        assert!(super::match_topic(topic, filter));

        let topic = "a/b/c/d/e/f";
        let filter = "a/b/c/#";
        assert!(super::match_topic(topic, filter));

        let topic = "a/b/c";
        let filter = "a/+/c";
        assert!(super::match_topic(topic, filter));
        let topic = "a/b/c/d/e";
        let filter = "a/+/c/+/e";
        assert!(super::match_topic(topic, filter));

        let topic = "a/b";
        let filter = "a/b/+";
        assert!(!super::match_topic(topic, filter));

        let filter1 = "a/b/+";
        let filter2 = "a/b/#";
        assert!(super::match_topic(filter1, filter2));
        assert!(!super::match_topic(filter2, filter1));

        let filter1 = "a/b/+";
        let filter2 = "#";
        assert!(super::match_topic(filter1, filter2));

        let filter1 = "a/+/c/d";
        let filter2 = "a/+/+/d";
        assert!(super::match_topic(filter1, filter2));
        assert!(!super::match_topic(filter2, filter1));
    }
    */
}
