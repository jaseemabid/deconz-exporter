use std::{collections::HashMap, error::Error};

use chrono::{DateTime, NaiveDateTime, Utc};
use prometheus::{labels, opts, GaugeVec, Registry, Result as PResult, TextEncoder};
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
    static ref REGISTRY: Registry = Registry::new_custom(Some("deconz".into()), None)
        .expect("Failed to create registry");

    static ref INFO: GaugeVec = GaugeVec::new(opts!("gateway_info", "Gateway static info"),
        &["name", "apiversion"]).unwrap();

    static ref BATTERY: GaugeVec = GaugeVec::new(opts!("battery", "Battery level in percentage"),
        &["manufacturername", "modelid", "name", "swversion"]).unwrap();

    static ref TEMPERATURE: GaugeVec = GaugeVec::new(opts!("temperature_celsius", "Temperature in degree Celsius"),
        &["manufacturername", "modelid", "name", "swversion", "type"]).unwrap();

    static ref PRESSURE: GaugeVec = GaugeVec::new(opts!("pressure_hpa", "Pressure in hPa"),
        &["manufacturername", "modelid", "name", "swversion", "type"]).unwrap();

    static ref HUMIDITY: GaugeVec = GaugeVec::new(opts!("humidity_ratio", "Relative humidity in percentage"),
        &["manufacturername", "modelid", "name", "swversion", "type"]).unwrap();

    static ref LASTUPDATED: GaugeVec = GaugeVec::new(opts!("sensor_last_updated_ms", "Duration since the sensor was last updated in ms"),
        &["manufacturername", "modelid", "name", "swversion"]).unwrap();

    static ref LASTSEEN: GaugeVec = GaugeVec::new(opts!("sensor_last_seen_ms", "Duration since the sensor was last seen in ms"),
        &["manufacturername", "modelid", "name", "swversion"]).unwrap();

}

/// deCONZ gateway config
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

// State change
//
// Depending on the websocketnotifyall setting: a map containing all or only the
// changed state attributes of a group, light, or sensor resource.  Only for
// changed events.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateChange {
    pub temperature: Option<i32>,
    pub humidity: Option<i32>,
    pub pressure: Option<i32>,
    // TODO: Unsure if this is local or UTC, there is no TZ on the timestamp :/
    pub lastupdated: NaiveDateTime,
}

/// Sensor info
///
/// See [device
/// spec](https://dresden-elektronik.github.io/deconz-rest-doc/devices/xiaomi/lumi_weather/)
/// for more info.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sensor {
    #[serde(default)]
    pub config: Option<SensorConfig>,
    pub etag: Option<String>,
    // Timestamp of the last power-cycle or rejoin.
    pub lastannounced: Option<DateTime<Utc>>,
    // Timestamp of the last communication.
    #[serde(with = "iso8601_without_seconds")]
    pub lastseen: DateTime<Utc>,
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

/// State carried around between events.
#[derive(Default)]
pub struct State {
    sensors: HashMap<String, Sensor>,
}

/// Websocket event from deCONZ for Conbee2
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
    pub state: Option<StateChange>,
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
type Callback = fn(&mut Event, &mut State) -> Result<(), Box<dyn Error>>;

/// Read gateway config from deCONZ REST API
fn gateway(host: &Url, username: &str) -> Result<Gateway, reqwest::Error> {
    let mut host = host.clone();
    host.set_path(&format!("/api/{}/config", username));
    info!("Connecting to API gateway at {host}");
    reqwest::blocking::get(host)?.json()
}

/// Discover websocket port from gateway config
fn websocket(host: &Url, username: &str) -> Result<Url, Box<dyn Error>> {
    let gw = gateway(host, username)?;

    INFO.with(&labels! {"name" =>  gw.name.as_str(), "apiversion" => gw.apiversion.as_str()})
        .set(1.0);

    let mut host = host.clone();

    host.set_scheme("ws").unwrap();
    host.set_port(Some(gw.websocketport)).unwrap();

    info!("Discovered websocket port at {}", host);
    Ok(host)
}

/// Run listener for websocket events.
pub fn run(host: &Url, username: &str) -> Result<(), Box<dyn Error>> {
    let socket = websocket(host, username)?;
    register_metrics()?;
    stream(&socket, &mut State::default(), process)
}

/// Run a callback for each event received over websocket.
//
// NOTE: A stream of Events would have been much neater than a callback, but Rust makes that API significantly more
// painful to implement.  Revisit this later.
fn stream(url: &Url, state: &mut State, callback: Callback) -> Result<(), Box<dyn Error>> {
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
fn process(e: &mut Event, state: &mut State) -> Result<(), Box<dyn Error>> {
    debug!("Received event. id:{} type: {}", e.id, e.type_);

    // Sensor attributes contains human friendly names and labels. Store them
    // now for future events with no attributes.
    if let Some(attr) = &e.attr && e.type_ == "event" && e.event == "changed" {
        debug!("Updating attrs for {}: {:?}", e.id, attr);
        state.sensors.insert(e.id.to_string(), attr.clone());

        LASTSEEN
            .with(&attr.labels(false))
            .set(attr.lastseen.timestamp_millis() as f64);

        return Ok(());
    }

    // State often has 2 keys, `lastupdated` and another one that is the actual data. Handle those, ignore the rest
    if let Some(change) = &e.state &&
       let Some(sensor) = state.sensors.get(&e.id) &&
       e.type_ == "event" && e.event == "changed" {

        debug!("Update state for sensor '{}': {:?}",  sensor.name, change);


        LASTUPDATED.with(&sensor.labels(false))
            .set(change.lastupdated.and_utc().timestamp_millis() as f64);

        if let Some(p) = change.pressure {
            PRESSURE.with(&sensor.labels(true)).set(p as f64);
        }

        // Xiomi Aqara sensors report the temperature as 2134 instead of
        // 21.34°C. Same for humidity. Scale it down.
        if let Some(t) = change.temperature {
            TEMPERATURE
                .with(&sensor.labels(true))
                .set(if t.abs() > 100 { t as f64 / 100.0 } else { t as f64 });
        }

        if let Some(h) = change.humidity {
            HUMIDITY
                .with(&sensor.labels(true))
                .set(if h.abs() > 100 { h as f64 / 100.0 } else { h as f64 });
        }

        return Ok(());
    }

    // Config change should be pretty much identical to state change
    if let Some(config) = &e.config && e.type_ == "event" && e.event == "changed" {
        if let Some(s) = state.sensors.get(&e.id) {
            debug!(
                "Updating battery for sensor '{}': {}",
                s.name, config.battery
            );
            BATTERY.with(&s.labels(false)).set(config.battery);
        } else {
            warn!("Unknown config change, ignoring it: {:?}", config)
        }
        return Ok(());
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

// Register metrics
fn register_metrics() -> PResult<()> {
    info!("Registering metrics",);
    REGISTRY.register(Box::new(INFO.clone()))?;
    REGISTRY.register(Box::new(BATTERY.clone()))?;
    REGISTRY.register(Box::new(TEMPERATURE.clone()))?;
    REGISTRY.register(Box::new(PRESSURE.clone()))?;
    REGISTRY.register(Box::new(HUMIDITY.clone()))?;
    REGISTRY.register(Box::new(LASTUPDATED.clone()))?;
    REGISTRY.register(Box::new(LASTSEEN.clone()))
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

mod iso8601_without_seconds {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%dT%H:%MZ";

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        #[allow(deprecated)]
        // DateTime::parse_from_str doesn't work with the custom format, so
        // stick with these deprecated APIs for now.
        Utc.datetime_from_str(&s, FORMAT)
            .map_err(serde::de::Error::custom)
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
        register_metrics().unwrap();
        let mut state = State::default();

        for (linum, event) in events
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
        {
            let mut e = serde_json::from_str::<Event>(event).unwrap_or_else(|err| {
                panic!(
                    "Failed to parse event in line {} \nEvent: {} \nError: {}",
                    linum + 1,
                    &event,
                    err
                )
            });

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
