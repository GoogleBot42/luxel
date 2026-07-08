//! Home Assistant MQTT integration: topic names, discovery payloads, and
//! command parsing — pure `no_std` string builders shared by the firmware
//! MQTT task and the native mirror, so the wire contract is identical and
//! unit-testable off-device.
//!
//! Entity model (HA MQTT discovery, <https://www.home-assistant.io/integrations/mqtt/>):
//!   - one `light` (JSON schema: power + brightness) per device
//!   - one `select` carrying the device pattern library by name
//! State/command topics live under `luxel/<id>/…`; discovery configs under
//! `homeassistant/…/config` (retained). `luxel/<id>/status` is the
//! availability topic (LWT: `offline`).

use alloc::string::String;
use alloc::vec::Vec;

use crate::jsonview::json_escape;

pub const ONLINE: &str = "online";
pub const OFFLINE: &str = "offline";

pub fn availability_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/status")
}
pub fn light_set_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/light/set")
}
pub fn light_state_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/light/state")
}
pub fn pattern_set_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/pattern/set")
}
pub fn pattern_state_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/pattern/state")
}
pub fn light_config_topic(id: &str) -> String {
    alloc::format!("homeassistant/light/{id}/light/config")
}
pub fn pattern_config_topic(id: &str) -> String {
    alloc::format!("homeassistant/select/{id}/pattern/config")
}

/// The `device` block shared by both discovery payloads — groups the
/// entities under one device card in HA.
fn device_block(id: &str, name: &str, version: &str) -> String {
    alloc::format!(
        "\"device\":{{\"identifiers\":[\"{}\"],\"name\":\"{}\",\"manufacturer\":\"Luxel\",\"model\":\"Luxel LED controller\",\"sw_version\":\"{}\"}}",
        json_escape(id),
        json_escape(name),
        json_escape(version)
    )
}

/// Discovery config for the light entity (JSON schema, brightness 0–255).
pub fn light_discovery_json(id: &str, name: &str, version: &str) -> String {
    alloc::format!(
        "{{\"schema\":\"json\",\"name\":\"Light\",\"unique_id\":\"{id}_light\",\"command_topic\":\"{}\",\"state_topic\":\"{}\",\"availability_topic\":\"{}\",\"brightness\":true,{}}}",
        light_set_topic(id),
        light_state_topic(id),
        availability_topic(id),
        device_block(id, name, version)
    )
}

/// Discovery config for the pattern select (options = library names).
pub fn pattern_discovery_json(id: &str, name: &str, version: &str, options: &[String]) -> String {
    let mut opts = String::from("[");
    for (i, o) in options.iter().enumerate() {
        if i > 0 {
            opts.push(',');
        }
        opts.push('"');
        opts.push_str(&json_escape(o));
        opts.push('"');
    }
    opts.push(']');
    alloc::format!(
        "{{\"name\":\"Pattern\",\"unique_id\":\"{id}_pattern\",\"command_topic\":\"{}\",\"state_topic\":\"{}\",\"availability_topic\":\"{}\",\"options\":{},{}}}",
        pattern_set_topic(id),
        pattern_state_topic(id),
        availability_topic(id),
        opts,
        device_block(id, name, version)
    )
}

pub fn diag_state_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/diag")
}
pub fn playlist_cmd_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/playlist/cmd")
}
pub fn playlist_state_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/playlist/state")
}
pub fn diag_config_topic(id: &str, which: &str) -> String {
    alloc::format!("homeassistant/sensor/{id}/{which}/config")
}
pub fn playlist_switch_config_topic(id: &str) -> String {
    alloc::format!("homeassistant/switch/{id}/playlist/config")
}
pub fn playlist_button_config_topic(id: &str, which: &str) -> String {
    alloc::format!("homeassistant/button/{id}/pl_{which}/config")
}

/// Diagnostic sensors: fps + free heap, one JSON state topic, extracted
/// with value_template. Marked as diagnostic entities so HA files them
/// under the device's diagnostics section.
pub fn diag_discovery_json(id: &str, name: &str, version: &str, which: &str) -> String {
    let (label, field, unit) = match which {
        "fps" => ("FPS", "fps", "\"unit_of_measurement\":\"fps\","),
        _ => ("Free heap", "heap", "\"unit_of_measurement\":\"B\","),
    };
    alloc::format!(
        "{{\"name\":\"{label}\",\"unique_id\":\"{id}_{field}\",\"state_topic\":\"{}\",\"value_template\":\"{{{{ value_json.{field} }}}}\",{unit}\"entity_category\":\"diagnostic\",\"availability_topic\":\"{}\",{}}}",
        diag_state_topic(id),
        availability_topic(id),
        device_block(id, name, version)
    )
}

/// The playlist as a switch (ON = auto-advancing) …
pub fn playlist_switch_discovery_json(id: &str, name: &str, version: &str) -> String {
    alloc::format!(
        "{{\"name\":\"Playlist\",\"unique_id\":\"{id}_playlist\",\"command_topic\":\"{}\",\"state_topic\":\"{}\",\"payload_on\":\"play\",\"payload_off\":\"stop\",\"state_on\":\"ON\",\"state_off\":\"OFF\",\"availability_topic\":\"{}\",{}}}",
        playlist_cmd_topic(id),
        playlist_state_topic(id),
        availability_topic(id),
        device_block(id, name, version)
    )
}

/// … plus next/previous buttons on the same command topic.
pub fn playlist_button_discovery_json(
    id: &str,
    name: &str,
    version: &str,
    which: &str,
) -> String {
    let label = if which == "next" { "Next pattern" } else { "Previous pattern" };
    alloc::format!(
        "{{\"name\":\"{label}\",\"unique_id\":\"{id}_pl_{which}\",\"command_topic\":\"{}\",\"payload_press\":\"{which}\",\"availability_topic\":\"{}\",{}}}",
        playlist_cmd_topic(id),
        availability_topic(id),
        device_block(id, name, version)
    )
}

pub fn diag_state_json(fps: u32, heap: u32) -> String {
    alloc::format!("{{\"fps\":{fps},\"heap\":{heap}}}")
}

/// Light state report. `brightness` is the HA 0–255 scale.
pub fn light_state_json(on: bool, brightness: u8) -> String {
    alloc::format!(
        "{{\"state\":\"{}\",\"brightness\":{}}}",
        if on { "ON" } else { "OFF" },
        brightness
    )
}

/// Device brightness (0–31, the SK9822 current field) → HA 0–255.
pub fn brightness_to_ha(dev: u8) -> u8 {
    ((dev.min(31) as u16 * 255 + 15) / 31) as u8
}

/// HA 0–255 → device 0–31 (nonzero HA never rounds to device 0 — HA treats
/// brightness 0 as OFF, so a tiny-but-on value must stay visibly on).
pub fn brightness_from_ha(ha: u8) -> u8 {
    let dev = ((ha as u16 * 31 + 127) / 255) as u8;
    if ha > 0 && dev == 0 {
        1
    } else {
        dev
    }
}

/// A parsed light command (HA JSON schema): either field may be absent.
#[derive(Default, Debug, PartialEq)]
pub struct LightCmd {
    pub on: Option<bool>,
    /// HA 0–255 scale.
    pub brightness: Option<u8>,
}

/// Parse an HA JSON-schema light command. Not a general JSON parser — a
/// targeted scan for the two fields HA sends (`state`, `brightness`),
/// tolerant of whitespace and field order.
pub fn parse_light_command(payload: &str) -> LightCmd {
    let mut cmd = LightCmd::default();
    if let Some(v) = scan_string_field(payload, "state") {
        cmd.on = Some(v.eq_ignore_ascii_case("ON"));
    }
    if let Some(v) = scan_number_field(payload, "brightness") {
        cmd.brightness = Some(v.clamp(0, 255) as u8);
    }
    cmd
}

fn field_value_start<'a>(payload: &'a str, field: &str) -> Option<&'a str> {
    let key = alloc::format!("\"{field}\"");
    let at = payload.find(&key)? + key.len();
    let rest = payload[at..].trim_start();
    rest.strip_prefix(':').map(str::trim_start)
}

fn scan_string_field<'a>(payload: &'a str, field: &str) -> Option<&'a str> {
    let rest = field_value_start(payload, field)?.strip_prefix('"')?;
    rest.split('"').next()
}

fn scan_number_field(payload: &str, field: &str) -> Option<i32> {
    let rest = field_value_start(payload, field)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Topics an MQTT session must subscribe to.
pub fn command_topics(id: &str) -> Vec<String> {
    alloc::vec![
        light_set_topic(id),
        pattern_set_topic(id),
        playlist_cmd_topic(id),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn topics_and_discovery() {
        assert_eq!(availability_topic("luxel-abc"), "luxel/luxel-abc/status");
        let d = light_discovery_json("luxel-abc", "Luxel abc", "0.1.19");
        assert!(d.contains("\"schema\":\"json\""));
        assert!(d.contains("\"command_topic\":\"luxel/luxel-abc/light/set\""));
        assert!(d.contains("\"brightness\":true"));
        assert!(d.contains("\"identifiers\":[\"luxel-abc\"]"));
        let p = pattern_discovery_json(
            "luxel-abc",
            "Luxel abc",
            "0.1.19",
            &["Rainbow".to_string(), "KITT \"eye\"".to_string()],
        );
        assert!(p.contains("\"options\":[\"Rainbow\",\"KITT \\\"eye\\\"\"]"));
    }

    #[test]
    fn extended_entities() {
        let d = diag_discovery_json("luxel-abc", "Luxel abc", "1.0", "fps");
        assert!(d.contains("\"value_template\":\"{{ value_json.fps }}\""));
        assert!(d.contains("\"entity_category\":\"diagnostic\""));
        assert!(d.contains("\"state_topic\":\"luxel/luxel-abc/diag\""));
        let s = playlist_switch_discovery_json("luxel-abc", "Luxel abc", "1.0");
        assert!(s.contains("\"payload_on\":\"play\""));
        assert!(s.contains("\"command_topic\":\"luxel/luxel-abc/playlist/cmd\""));
        let b = playlist_button_discovery_json("luxel-abc", "Luxel abc", "1.0", "next");
        assert!(b.contains("\"payload_press\":\"next\""));
        assert_eq!(diag_state_json(120, 45000), "{\"fps\":120,\"heap\":45000}");
        assert_eq!(command_topics("x").len(), 3);
    }

    #[test]
    fn light_command_parsing() {
        assert_eq!(
            parse_light_command("{\"state\":\"ON\"}"),
            LightCmd { on: Some(true), brightness: None }
        );
        assert_eq!(
            parse_light_command("{ \"brightness\" : 128 , \"state\" : \"OFF\" }"),
            LightCmd { on: Some(false), brightness: Some(128) }
        );
        assert_eq!(parse_light_command("{}"), LightCmd::default());
    }

    #[test]
    fn brightness_scale_round_trips() {
        assert_eq!(brightness_to_ha(0), 0);
        assert_eq!(brightness_to_ha(31), 255);
        assert_eq!(brightness_from_ha(255), 31);
        assert_eq!(brightness_from_ha(0), 0);
        assert_eq!(brightness_from_ha(1), 1); // tiny-but-on stays on
        for d in 0..=31u8 {
            assert_eq!(brightness_from_ha(brightness_to_ha(d)), d);
        }
    }
}
