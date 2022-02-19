use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ConBeeII gateway config
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
}

pub type Sensors = HashMap<u16, Sensor>;

/// Read config from ConBee II REST API
pub async fn config<'a>(host: &str, username: &str) -> Result<Gateway, reqwest::Error> {
    reqwest::get(format!("{}/api/{}/config", host, username))
        .await?
        .json()
        .await
}

/// Read sensor from ConBee II REST API
pub async fn sensors<'a>(host: &str, username: &str) -> Result<Sensors, reqwest::Error> {
    reqwest::get(format!("{}/api/{}/sensors", host, username))
        .await?
        .json()
        .await
}

/// Authenticate with ConBee II device
mod auth {}

#[cfg(test)]
mod test {
    use super::*;

    const HOST: &str = "http://nyx.jabid.in:4501";
    const USERNAME: &str = "381412B455";

    #[tokio::test]
    async fn test_config() {
        let resp = config(HOST, USERNAME).await;

        match resp {
            Ok(cfg) => {
                // println!("Got config {:#?}", cfg);

                assert_eq!(cfg.apiversion, "1.16.0");
                assert_eq!(cfg.bridgeid, "00212EFFFF07D25D")
            }
            Err(e) => {
                panic!("Failed to read gateway config from home assistant: {}", e)
            }
        }
    }

    #[tokio::test]
    async fn test_sensors() {
        let resp = sensors(HOST, USERNAME).await;

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
    fn test_serde_sensor() {
        let data = r#"
        {
            "config": {
              "battery": 100,
              "offset": 0,
              "on": true,
              "reachable": true
            },
            "ep": 1,
            "etag": "e8a1e47355a41c2f0d7d0481e7377961",
            "lastannounced": null,
            "lastseen": "2022-02-19T16:14Z",
            "manufacturername": "LUMI",
            "modelid": "lumi.weather",
            "name": "Office",
            "state": {
              "humidity": 4917,
              "lastupdated": "2022-02-19T16:14:15.172"
            },
            "swversion": "20191205",
            "type": "ZHAHumidity",
            "uniqueid": "00:15:8d:00:07:e0:83:5b-01-0405"
          }"#;

        assert!(serde_json::from_str::<Sensor>(data).is_ok());
    }

    #[test]
    fn test_serde_socket_events() {
        let events = r#"
{"e":"changed","id":"6","r":"sensors","state":{"humidity":4610,"lastupdated":"2022-02-19T21:37:44.737"},"t":"event","uniqueid":"00:15:8d:00:07:e0:ac:42-01-0405"}
{"e":"changed","id":"7","r":"sensors","state":{"lastupdated":"2022-02-19T21:37:44.740","pressure":1003},"t":"event","uniqueid":"00:15:8d:00:07:e0:ac:42-01-0403"}
{"attr":{"id":"2","lastannounced":null,"lastseen":"2022-02-19T21:38Z","manufacturername":"LUMI","modelid":"lumi.weather","name":"Living room","swversion":"20191205","type":"ZHATemperature","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0402"},"e":"changed","id":"2","r":"sensors","t":"event","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0402"}
{"e":"changed","id":"2","r":"sensors","state":{"lastupdated":"2022-02-19T21:38:11.933","temperature":2090},"t":"event","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0402"}
{"attr":{"id":"3","lastannounced":null,"lastseen":"2022-02-19T21:38Z","manufacturername":"LUMI","modelid":"lumi.weather","name":"Living room","swversion":"20191205","type":"ZHAHumidity","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0405"},"e":"changed","id":"3","r":"sensors","t":"event","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0405"}
{"attr":{"id":"4","lastannounced":null,"lastseen":"2022-02-19T21:38Z","manufacturername":"LUMI","modelid":"lumi.weather","name":"Living room","swversion":"20191205","type":"ZHAPressure","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0403"},"e":"changed","id":"4","r":"sensors","t":"event","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0403"}
{"e":"changed","id":"3","r":"sensors","state":{"humidity":5434,"lastupdated":"2022-02-19T21:38:11.940"},"t":"event","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0405"}
{"e":"changed","id":"4","r":"sensors","state":{"lastupdated":"2022-02-19T21:38:11.944","pressure":1003},"t":"event","uniqueid":"00:15:8d:00:07:e0:a8:15-01-0403"}
{"attr":{"id":"1","lastannounced":null,"lastseen":"2022-02-19T21:38Z","manufacturername":"dresden elektronik","modelid":"ConBee II","name":"Configuration tool 1","swversion":"0x26660700","type":"Configuration tool","uniqueid":"00:21:2e:ff:ff:07:d2:5d-01"},"e":"changed","id":"1","r":"lights","t":"event","uniqueid":"00:21:2e:ff:ff:07:d2:5d-01"}
{"attr":{"id":"1","lastannounced":null,"lastseen":"2022-02-19T21:39Z","manufacturername":"dresden elektronik","modelid":"ConBee II","name":"Configuration tool 1","swversion":"0x26660700","type":"Configuration tool","uniqueid":"00:21:2e:ff:ff:07:d2:5d-01"},"e":"changed","id":"1","r":"lights","t":"event","uniqueid":"00:21:2e:ff:ff:07:d2:5d-01"}
          "#;

        for event in events.lines().filter(|l| !l.trim().is_empty()) {
            serde_json::from_str::<Event>(event)
                .unwrap_or_else(|err| panic!("Failed to parse event {}: {}", &event, err));
        }
    }
}
