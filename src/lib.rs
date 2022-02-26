#[macro_use]
extern crate lazy_static;

use prometheus::{GaugeVec, Opts, Registry, TextEncoder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, error::Error};

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
    pub websocketport: u32,
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

/// Sensor info key'ed by ID
pub type Sensors = HashMap<String, Sensor>;

/// Callback function executed for every update event
pub type Callback = fn(&mut Event, &mut Sensors) -> Result<(), Box<dyn Error>>;

/// Read gateway config from ConBee II REST API
pub fn gateway(host: &str, username: &str) -> Result<Gateway, reqwest::Error> {
    reqwest::blocking::get(format!("{}/api/{}/config", host, username))?.json()
}

/// Read sensor from ConBee II REST API
pub fn sensors(host: &str, username: &str) -> Result<Sensors, reqwest::Error> {
    reqwest::blocking::get(format!("{}/api/{}/sensors", host, username))?.json()
}

/// Run listener for websocket events.
pub fn run(url: &str) -> Result<(), Box<dyn Error>> {
    info!("🔌 Start listening for websocket events at {url}");

    // State machine for event update data
    let mut sensors: Sensors = Default::default();
    stream(url, &mut sensors, process)
}

/// Run a callback for each event received over websocket.
//
// NOTE: A stream of Events would have been much neater than a callback, but Rust makes that API significantly more
// painful to implement.  Revisit this later.
//
pub fn stream(url: &str, state: &mut Sensors, callback: Callback) -> Result<(), Box<dyn Error>> {
    let (mut socket, _) = tungstenite::client::connect(url)?; // TODO: What's the second return argument here?

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
pub fn process(e: &mut Event, state: &mut Sensors) -> Result<(), Box<dyn Error>> {
    debug!("Received event for {}", e.id);

    let labels = vec!["manufacturername", "modelid", "name", "swversion", "type"];

    // Sensor attributes contains human friendly names and labels. Store them now for future events with no attributes.
    if let Some(attr) = &e.attr {
        if e.type_ == "event" && e.event == "changed" {
            state.insert(e.id.to_string(), attr.clone());
            return Ok(());
        }
    }

    // State often has 2 keys, `lastupdated` and another one that is the actual data. Handle those, ignore the rest
    if e.type_ == "event" && e.event == "changed" && !e.state.is_empty() {
        if let Some(sensor) = state.get(&e.id) {
            for (k, v) in &e.state {
                if k == "lastupdated" {
                    continue;
                }

                if let Some(val) = v.as_f64() {
                    debug!("Updating metric ID:{}, {k}:{v}", e.id);
                    let opts = Opts::new(k, format!("Generic {} metric", k));
                    let gauge = GaugeVec::new(opts, &labels).unwrap();
                    // Register metric and ignore duplicates since we have no way of knowing all the metrics upfront.
                    match REGISTRY.register(Box::new(gauge.clone())) {
                        Ok(()) | Err(prometheus::Error::AlreadyReg) => {}
                        Err(err) => return Err(err.into()),
                    }
                    gauge.with(&sensor.labels()).set(val);
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
            let s = state.get(&e.id).unwrap().clone();

            let opts = Opts::new("battery", "Sensor battery level");
            let gauge = GaugeVec::new(opts, &labels).unwrap();

            // Register metric and ignore duplicates
            match REGISTRY.register(Box::new(gauge.clone())) {
                Ok(()) | Err(prometheus::Error::AlreadyReg) => {}
                Err(err) => return Err(err.into()),
            }

            debug!("Updating metric ID:{}, battery:{}", e.id, config.battery);
            gauge.with(&s.labels()).set(config.battery);

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

impl Sensor {
    /// Convert sensor into prometheus labels
    fn labels(&self) -> HashMap<&str, &str> {
        vec![
            ("manufacturername", &self.manufacturername),
            ("modelid", &self.modelid),
            ("name", &self.name),
            ("swversion", self.swversion.as_ref().unwrap_or(&self.dummy)),
            ("type", &self.tipe),
        ]
        .into_iter()
        .map(|(name, value)| (name, value.as_str()))
        .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const HOST: &str = "http://nyx.jabid.in:4501";
    const WS: &str = "ws://nyx.jabid.in:4502";
    const USERNAME: &str = "381412B455";

    #[test]
    fn read_config() {
        let resp = gateway(HOST, USERNAME);

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
    fn read_sensors() {
        let resp = sensors(HOST, USERNAME);

        match resp {
            Ok(cfg) => {
                //println!("Got config {:#?}", cfg);
                assert!(cfg.len() > 1, "Didn't get any sensor info");
            }
            Err(e) => {
                panic!("Failed to read sensor config from home assistant: {}", e)
            }
        }
    }

    #[test]
    //#[ignore]
    fn read_stream() {
        stream(WS, &mut Default::default(), |_, _| Ok(())).unwrap();
    }

    #[test]
    fn test_process() {
        let events = include_str!("../events.json");
        let mut state: Sensors = Default::default();

        for event in events.lines().filter(|l| !l.trim().is_empty()) {
            let mut e = serde_json::from_str::<Event>(event)
                .unwrap_or_else(|err| panic!("Failed to parse event {}: {}", &event, err));

            process(&mut e, &mut state)
                .unwrap_or_else(|err| panic!("Failed to process event {:?}: {}", &e, err));
        }
    }

    #[test]
    fn serde_sensor() {
        let data = r#"
        {
            "config": { "battery": 100, "offset": 0, "on": true, "reachable": true },
            "ep": 1,
            "etag": "e8a1e47355a41c2f0d7d0481e7377961",
            "lastannounced": null,
            "lastseen": "2022-02-19T16:14Z",
            "manufacturername": "LUMI",
            "modelid": "lumi.weather",
            "name": "Office",
            "state": { "humidity": 4917, "lastupdated": "2022-02-19T16:14:15.172" },
            "swversion": "20191205",
            "type": "ZHAHumidity",
            "uniqueid": "00:15:8d:00:07:e0:83:5b-01-0405"
        }"#;

        assert!(serde_json::from_str::<Sensor>(data).is_ok());
    }
}
