use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;

#[cfg(not(test))]
use log::{debug, warn};

#[cfg(test)]
use std::{println as debug, println as warn};

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
#[derive(Serialize, Deserialize, Debug)]
pub struct Sensor {
    pub config: HashMap<String, Value>,
    pub etag: Option<String>,
    pub lastannounced: Option<String>,
    pub lastseen: Option<String>,
    pub manufacturername: String,
    pub modelid: String,
    pub name: String,
    pub state: HashMap<String, Value>,
    pub swversion: Option<String>,
    #[serde(rename = "type")]
    pub tipe: String,
    pub uniqueid: String,
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
    pub config: HashMap<String, Value>,
    // The (new) name of a resource.  Only for changed events.
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
    #[serde(default)]
    pub attr: HashMap<String, Value>,
}

/// Sensor info key'ed by ID
pub type Sensors = HashMap<u16, Sensor>;

/// Callback function executed for every update event
pub type Callback = fn(&mut Event) -> Result<(), Box<dyn Error>>;

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
    stream(url, process)
}

/// Run a callback for each event received over websocket.
//
// NOTE: A stream of Events would have been much neater than a callback, but Rust makes that API significantly more
// painful to implement.  Revisit this later.
pub fn stream(url: &str, callback: Callback) -> Result<(), Box<dyn Error>> {
    let (mut socket, _) = tungstenite::client::connect(url)?; // TODO: What's the second return argument here?

    loop {
        let message = socket.read_message()?;
        match serde_json::from_str::<Event>(message.clone().into_text().unwrap().as_str()) {
            Ok(mut event) => {
                if let Err(err) = callback(&mut event) {
                    warn!("Failed to handle event: `{:?}`: {:?}", event, err)
                }
            }
            Err(err) => {
                warn!("Failed to serialize, ignoring `{}`: {:?}", message, err)
            }
        }
    }
}

/// Process events that can be handled and throw away everything else with a warning log.
fn process(e: &mut Event) -> Result<(), Box<dyn Error>> {
    let Event {
        type_,
        event,
        id,
        state,
        ..
    } = &e;

    // 1. Events with attrs, use this to get sensor info
    // 2. Events with state, use this to get real data

    // State often has 2 keys, `lastupdated` and another one that is the actual data. Handle those, ignore the rest
    if type_ == "event" && event == "changed" && !state.is_empty() {
        for (k, v) in state {
            if k == "lastupdated" {
                continue;
            }

            debug!("Update metric {id}, {k} = {v}");
        }
    }

    warn!("Ignoring unknown event {:?}", e);

    Ok(())
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
        stream(WS, |_| Ok(())).unwrap();
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

    #[test]
    fn serde_socket_events() {
        let events = include_str!("../events.json");
        for event in events.lines().filter(|l| !l.trim().is_empty()) {
            serde_json::from_str::<Event>(event)
                .unwrap_or_else(|err| panic!("Failed to parse event {}: {}", &event, err));
        }
    }
}
