# 🚀 deconz-exporter

A very simple (and naive) Prometheus exporter for [deCONZ Phoscon][phoscon] zigbee gateway.
Exports prometheus metrics for sensors connected to [Conbee II][conbee2] USB gateway.

![Example screenshot](./screenshot.png)

## 📈 Exported metrics

```
# HELP deconz_gateway_info Gateway static info
# TYPE deconz_gateway_info gauge
deconz_gateway_info{apiversion, name}

# HELP deconz_battery Battery level of sensors
# TYPE deconz_battery gauge
deconz_battery{id, manufacturername, modelid, name, swversion}

# HELP deconz_humidity Relative humidity in percentage
# TYPE deconz_humidity gauge
deconz_humidity_ratio{id, manufacturername, modelid, name, swversion, type}

# HELP deconz_pressure Pressure in hPa
# TYPE deconz_pressure gauge
deconz_pressure_hpa{id, manufacturername,modelid, name, swversion, type}

# HELP deconz_temperature Temperature in degree Celsius
# TYPE deconz_temperature gauge
deconz_temperature_celsius{id, manufacturername, modelid, name, swversion, type}
```

## 🚲 Getting started

1. Enable discovery in gateway settings

   ![Enable discovery](./discovery.png)

1. Generate a new username for the exporter

   ```bash
   $ curl -X POST -s http://deconz:4501/api -d '{"devicetype": "deconz-exporter"}' | jq

   [{"success":{"username":"0E87CDA111"}}]
   ```

   Save the returned username as `DECONZ_API_USERNAME`.

1. Start the exporter.

   ### Using Docker

   ```bash
   docker run -p 9199:9199 \
     -e DECONZ_API_URL=http://deconz:4501 \
     -e DECONZ_API_USERNAME=0E87CDA111 \
     -e DECONZ_PORT=9199 \
     ghcr.io/jaseemabid/deconz-exporter:latest

   # Optional: override websocket URL
   docker run -p 9199:9199 \
     -e DECONZ_API_URL=http://deconz:4501 \
     -e DECONZ_API_USERNAME=0E87CDA111 \
     -e DECONZ_WS_URL=ws://deconz:4502 \
     -e DECONZ_PORT=9199 \
     ghcr.io/jaseemabid/deconz-exporter:latest

   # Optional: archive raw websocket events to a JSONL file
   docker run -p 9199:9199 \
     -e DECONZ_API_URL=http://deconz:4501 \
     -e DECONZ_API_USERNAME=0E87CDA111 \
     -e DECONZ_EVENTS_FILE=/data/events.jsonl \
     -v /path/to/host/dir:/data \
     ghcr.io/jaseemabid/deconz-exporter:latest
   ```

   ### Using Cargo

   ```bash
   # Using flags
   cargo run -- --api-url $DECONZ_API_URL --username $DECONZ_API_USERNAME --port 9199
   # Optional: override websocket URL
   cargo run -- --api-url $DECONZ_API_URL --username $DECONZ_API_USERNAME --ws-url ws://deconz:4502 --port 9199

   # Using env vars (supported by clap)
   DECONZ_API_URL=http://deconz:4501 \
   DECONZ_API_USERNAME=0E87CDA111 \
   DECONZ_WS_URL=ws://deconz:4502 \
   DECONZ_PORT=9199 \
   cargo run

   # Optional: archive raw websocket events to a JSONL file
   cargo run -- --api-url $DECONZ_API_URL --username $DECONZ_API_USERNAME --events-file /tmp/events.jsonl
   ```

1. Optionally override websocket url.

   The exporter will try to discover the websocket url by default from the API url,
   but use `--ws-url ws://deconz:4502` or `DECONZ_WS_URL=ws://deconz:4502` if you
   need to explicitly use a different url.

1. Optionally archive raw events.

   Use `--events-file /path/to/events.jsonl` or `DECONZ_EVENTS_FILE=/path/to/events.jsonl`
   to write every raw websocket event to a [JSONL] file (one JSON object per line).
   The file is opened in append mode, so events accumulate across restarts.

5. Profit! 🥇

## ⚙️ How does this work?

1. The exporter must be configured with a valid username and url to connect to [deCONZ REST API].
1. The websocket port is discovered though the REST API.
1. The [Websocket API] provides streaming updates to the exporter, which gets converted to metrics.

## 🕵️‍♂️ Debugging tips

1. [websocat] is a handy tool to see the raw websocket events emitted. Use it to debug issues, capture some sample
   events etc.

   ```
   $ websocat ws://nyx.jabid.in:4502

   {"attr":{"id":"1","lastannounced":null,"lastseen":"2022-03-04T22:42Z","manufacturername":"dresden elektronik","modelid":...
   ```

1. Run `$ cargo test` just to be sure.

## 📝 Notes

1. This exporter is only tested with a few devices I own. There is no guarantee that it would work with anything else.
1. Feel free to send me PRs for [other devices supported][compatibility] by [Conbee II][conbee2]
1. The auth flow is cumbersome and manual, would be great to automate this.
1. Reconnect cleanly on websocket errors, right now the exporter just dies and gets restarted
1. Process metrics are missing, should bring them back.
1. Auto discovery of gateways might be nice.
1. Add metrics on the number of events handled

## ⚖️ License

[MIT](https://choosealicense.com/licenses/mit)

[compatibility]: https://phoscon.de/en/conbee2/compatible
[conbee2]: https://phoscon.de/en/conbee2
[deconz rest api]: https://dresden-elektronik.github.io/deconz-rest-doc
[phoscon]: https://phoscon.de/en/conbee2/software#phoscon-app
[websocat]: https://github.com/vi/websocat
[jsonl]: https://jsonlines.org
[websocket api]: https://dresden-elektronik.github.io/deconz-rest-doc/endpoints/websocket
