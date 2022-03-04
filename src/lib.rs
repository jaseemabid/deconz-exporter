use std::{collections::HashMap, error::Error};

use prometheus::{
    core::Collector, opts, register_gauge_vec_with_registry, GaugeVec, Registry, TextEncoder,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

#[macro_use]
extern crate lazy_static;

#[cfg(not(test))]
use log::{debug, info, warn};

#[cfg(test)]
use std::{println as debug, println as warn, println as info};

lazy_static! {
    /// Global prometheus registry for all metrics
    static ref REGISTRY: Registry = Registry::new_custom(Some("deconz".into()), None).expect("Failed to create registry");
}

/// ConBeeII gateway config
#[derive(Serialize, Deserialize, Debug)]
pub struct Gateway {
    pub apiversion: String,
    pub bridgeid: String,
    pub devicename: String,
    pub dhcp: bool,
    pub gateway: String,
    pub ipaddress: String,
    pub linkbutton: bool,
    pub mac: String,
    pub modelid: String,
    pub name: String,
    pub swversion: String,
    pub websocketport: u16,
    pub zigbeechannel: u8,
}

/// Sensor config
///
/// Present only for "ZHA{Humidity, Pressure, Switch, Temperature}, null for "Configuration tool"
#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct SensorConfig {
    pub battery: f64,
    pub offset: f64,
    pub on: bool,
    pub reachable: bool,
}

/// Sensor info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sensor {
    #[serde(default)]
    pub config: Option<SensorConfig>,
    pub etag: Option<String>,
    pub lastannounced: Option<String>,
    pub lastseen: Option<String>,
    pub manufacturername: String,
    pub modelid: String,
    pub name: String,
    #[serde(default)]
    pub state: HashMap<String, Value>,
    pub swversion: Option<String>,
    #[serde(rename = "type")]
    pub tipe: String,
    pub uniqueid: String,
    #[serde(skip)]
    dummy: String,
}

pub struct State {
    sensors: HashMap<String, Sensor>,
    metrics: HashMap<String, GaugeVec>,
}

/// Websocket event from Conbee2
//
// https://dresden-elektronik.github.io/deconz-rest-doc/endpoints/websocket/#message-fields
#[derive(Serialize, Deserialize, Debug)]
pub struct Event {
    // "event" - the message holds an event.
    #[serde(rename = "t")]
    pub type_: String,
    // One of added | changed | deleted | scene-called
    #[serde(rename = "e")]
    pub event: String,
    // Resource is one of groups | lights | scenes | sensors
    #[serde(rename = "r")]
    pub resource: String,
    // The id of the resource to which the message relates
    pub id: String,
    // The uniqueid of the resource to which the message relates
    pub uniqueid: String,
    // The group id of the resource to which the message relates.
    pub gid: Option<String>,
    // The scene id of the resource to which the message relates.
    pub scid: Option<String>,
    // Depending on the `websocketnotifyall` setting: a map containing all or only the changed config attributes of a
    // sensor resource.  Only for changed events.
    #[serde(default)]
    pub config: Option<SensorConfig>,
    // The (new) name of a resource. Only for changed events.
    pub name: Option<String>,
    // Depending on the websocketnotifyall setting: a map containing all or only the changed state attributes of a
    // group, light, or sensor resource.  Only for changed events.
    #[serde(default)]
    pub state: HashMap<String, Value>,
    // The full group resource.  Only for added events of a group resource
    #[serde(default)]
    pub group: HashMap<String, Value>,
    // The full light resource.  Only for added events of a light resource.
    #[serde(default)]
    pub light: HashMap<String, Value>,
    // The full sensor resource.  Only for added events of a sensor resource.
    #[serde(default)]
    pub sensor: HashMap<String, Value>,
    // Undocumented, but present in API responses.
    pub attr: Option<Sensor>,
}

/// Callback function executed for every update event
pub type Callback = fn(&mut Event, &mut State) -> Result<(), Box<dyn Error>>;

/// Read gateway config from ConBee II REST API
pub fn gateway(host: &Url, username: &str) -> Result<Gateway, reqwest::Error> {
    let u = format!("{}/api/{}/config", host, username);
    info!("Connecting to API gateway at {u}");
    reqwest::blocking::get(u)?.json()
}

/// Discover websocket port from gateway config
pub fn websocket(host: &Url, username: &str) -> Result<Url, Box<dyn Error>> {
    let gw = gateway(host, username)?;
    let mut host = host.clone();

    host.set_scheme("ws").unwrap();
    host.set_port(Some(gw.websocketport)).unwrap();

    info!("Discovered websocket port at {}", host);
    Ok(host)
}

/// Run listener for websocket events.
pub fn run(host: &Url, username: &str) -> Result<(), Box<dyn Error>> {
    let mut state = State::with_metrics();
    let socket = websocket(host, username)?;
    stream(&socket, &mut state, process)
}

/// Run a callback for each event received over websocket.
//
// NOTE: A stream of Events would have been much neater than a callback, but Rust makes that API significantly more
// painful to implement.  Revisit this later.
//
pub fn stream(url: &Url, state: &mut State, callback: Callback) -> Result<(), Box<dyn Error>> {
    info!("🔌 Start listening for websocket events at {url}");

    let (mut socket, _) = tungstenite::client::connect(url)?;
    loop {
        match serde_json::from_str::<Event>(socket.read_message()?.to_text()?) {
            Ok(mut event) => {
                // Failing to process a single event is alright, and this process should just continue. Non recoverable
                // errors should bubble up so that the whole stream can be reestablished.
                if let Err(err) = callback(&mut event, state) {
                    warn!("Failed to handle event: `{:?}`: {:?}", event, err)
                }
            }
            Err(err) => {
                warn!("Failed to serialize, ignoring message: {:?}", err)
            }
        }
    }
}

/// Process events that can be handled and throw away everything else with a warning log.
///
/// The events structure is a bit messy and not in a good shape. See documentation of `Event` for details.
///
/// Events with `attrs` are used to get human readable labels and stored in a static map for future lookup, when state
/// updates arrive without these attributes.
pub fn process(e: &mut Event, state: &mut State) -> Result<(), Box<dyn Error>> {
    debug!("Received event for {}", e.id);

    // Sensor attributes contains human friendly names and labels. Store them now for future events with no attributes.
    if let Some(attr) = &e.attr {
        if e.type_ == "event" && e.event == "changed" {
            debug!("Updating attrs for {}", e.id);
            state.sensors.insert(e.id.to_string(), attr.clone());
            return Ok(());
        }
    }

    // State often has 2 keys, `lastupdated` and another one that is the actual data. Handle those, ignore the rest
    if e.type_ == "event" && e.event == "changed" && !e.state.is_empty() {
        if let Some(sensor) = state.sensors.get(&e.id) {
            for (k, v) in &e.state {
                if k == "lastupdated" {
                    continue;
                }

                if let Some(gauge) = state.metrics.get(k.as_str()) {
                    if let Some(val) = v.as_f64() {
                        debug!("Updating metric ID:{}, {k}:{v}", e.id);
                        gauge.with(&sensor.labels(true)).set(val);
                    }
                } else {
                    debug!("Ignoring metric ID:{}, {k}:{v}", e.id);
                }
                return Ok(());
            }
        } else {
            warn!("Ignoring event update for unknown sensor {}: {:?}", e.id, e)
        }

        return Ok(());
    }

    // Config change should be pretty much identical to state change
    if let Some(config) = &e.config {
        if e.type_ == "event" && e.event == "changed" {
            debug!("Updating metric ID:{}, battery:{}", e.id, config.battery);

            let s = state.sensors.get(&e.id).unwrap().clone();
            let gauge = state.metrics.get("battery").unwrap();

            gauge.with(&s.labels(false)).set(config.battery);
            return Ok(());
        }
    }

    warn!("Ignoring unknown event {:?}", e);

    Ok(())
}

/// Export prometheus metrics as a string
pub fn metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    encoder.encode_to_string(&metric_families).unwrap()
}

impl State {
    fn with_metrics() -> Self {
        let mut s = State {
            metrics: Default::default(),
            sensors: Default::default(),
        };

        let metrics = vec![
            register_gauge_vec_with_registry!(
                opts!("battery", "Battery level of sensors"),
                &["manufacturername", "modelid", "name", "swversion"],
                REGISTRY
            )
            .unwrap(),
            register_gauge_vec_with_registry!(
                opts!("humidity", "Humidity level"),
                &["manufacturername", "modelid", "name", "swversion", "type"],
                REGISTRY,
            )
            .unwrap(),
            register_gauge_vec_with_registry!(
                opts!("pressure", "Pressure level"),
                &["manufacturername", "modelid", "name", "swversion", "type"],
                REGISTRY
            )
            .unwrap(),
            register_gauge_vec_with_registry!(
                opts!("temperature", "Temperature level"),
                &["manufacturername", "modelid", "name", "swversion", "type"],
                REGISTRY,
            )
            .unwrap(),
        ];

        for gauge in metrics {
            s.metrics
                .insert(gauge.desc()[0].fq_name.clone(), gauge.clone());
        }

        s
    }
}

impl Sensor {
    /// Convert sensor into prometheus labels
    fn labels(&self, tipe: bool) -> HashMap<&str, &str> {
        vec![
            ("manufacturername", &self.manufacturername),
            ("modelid", &self.modelid),
            ("name", &self.name),
            ("swversion", self.swversion.as_ref().unwrap_or(&self.dummy)),
            if tipe {
                ("type", &self.tipe)
            } else {
                ("", &self.dummy)
            },
        ]
        .into_iter()
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, value)| (name, value.as_str()))
        .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[ignore]
    fn read_config() {
        let resp = gateway(
            &Url::parse("http://nyx.jabid.in:4501").unwrap(),
            "381412B455",
        );

        match resp {
            Ok(cfg) => {
                assert_eq!(cfg.apiversion, "1.16.0");
                assert_eq!(cfg.bridgeid, "00212EFFFF07D25D")
            }
            Err(e) => {
                panic!("Failed to read gateway config from home assistant: {}", e)
            }
        }
    }

    #[test]
    fn test_process() {
        let events = include_str!("../events.json");
        let mut state = State::with_metrics();

        for event in events.lines().filter(|l| !l.trim().is_empty()) {
            let mut e = serde_json::from_str::<Event>(event)
                .unwrap_or_else(|err| panic!("Failed to parse event {}: {}", &event, err));

            process(&mut e, &mut state)
                .unwrap_or_else(|err| panic!("Failed to process event {:?}: {}", &e, err));
        }

        // Now that all the data is handled, make sure metrics are present.
        let m = metrics();
        let m = m
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>();

        dbg!(&m);

        assert!(m.len() > 10, "Too few metrics exported")
    }
}
