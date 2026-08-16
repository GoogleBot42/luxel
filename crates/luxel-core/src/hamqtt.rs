//! Home Assistant MQTT integration: topic names, discovery payloads, and
//! command parsing — pure `no_std` string builders shared by the firmware
//! MQTT task and the native mirror, so the wire contract is identical and
//! unit-testable off-device. User-facing topic reference: docs/mqtt.md
//! (keep it in sync with this file).
//!
//! Entity model (HA MQTT discovery, <https://www.home-assistant.io/integrations/mqtt/>):
//!   - one `light` (JSON schema: power + brightness) per device
//!   - one `select` carrying the device pattern library by name
//! State/command topics live under `luxel/<id>/…`; discovery configs under
//! `homeassistant/…/config` (retained). `luxel/<id>/status` is the
//! availability topic (LWT: `offline`).
//!
//! `luxel/<id>/event` is a command-only topic with no HA entity: each
//! payload line is one injected pattern event (`type x y value`, see
//! [`parse_event_lines`]) — the MQTT face of the `readEvent()` surface.

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
pub fn event_topic(id: &str) -> String {
    alloc::format!("luxel/{id}/event")
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

/// Parse a plain decimal ("-12.5", ".25", "3") into `Fx` with integer
/// math only — keeps this no_std path independent of core's dec2flt
/// (~19 KB of flash) staying resident in the firmware image, which today
/// it does only incidentally (other code happens to parse floats). No
/// exponent form; magnitudes beyond the 16.16 range clamp.
fn parse_fx(s: &str) -> Option<crate::fixed::Fx> {
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (ip, fp) = s.split_once('.').unwrap_or((s, ""));
    if (ip.is_empty() && fp.is_empty())
        || !ip.bytes().all(|b| b.is_ascii_digit())
        || !fp.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let int: u64 = if ip.is_empty() { 0 } else { ip.parse().ok()? };
    let (mut num, mut den) = (0u64, 1u64);
    for b in fp.bytes().take(9) {
        num = num * 10 + (b - b'0') as u64;
        den *= 10;
    }
    let raw = int
        .saturating_mul(65536)
        .saturating_add((num * 65536 + den / 2) / den)
        .min(i32::MAX as u64) as i32;
    Some(crate::fixed::Fx::from_raw(if neg { -raw } else { raw }))
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
        event_topic(id),
    ]
}

/// Parse a `luxel/<id>/event` payload into `[type, x, y, value]` quads
/// for the engine event queue (the `readEvent()` surface, see netin's
/// `EV1` frame for the binary HTTP twin).
///
/// Text, automation-friendly: one event per line, whitespace-separated
/// decimal numbers `type [x [y [value]]]` — missing x/y default to 0,
/// missing value to 1, so an HA automation can publish just `"1"`.
/// Blank lines and lines that don't start with a number are skipped;
/// at most [`crate::netin::EV_MAX_BATCH`] events per payload.
pub fn parse_event_lines(payload: &str) -> Vec<[crate::fixed::Fx; 4]> {
    use crate::fixed::Fx;
    let mut out = Vec::new();
    for line in payload.lines() {
        if out.len() >= crate::netin::EV_MAX_BATCH {
            break;
        }
        let mut nums = line.split_whitespace().map(parse_fx);
        let Some(Some(ty)) = nums.next() else {
            continue; // blank or non-numeric line
        };
        let mut ev = [ty, Fx::ZERO, Fx::ZERO, Fx::ONE];
        for slot in ev.iter_mut().skip(1) {
            match nums.next() {
                Some(Some(v)) => *slot = v,
                Some(None) => break, // trailing garbage: keep what parsed
                None => break,
            }
        }
        out.push(ev);
    }
    out
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
        assert_eq!(command_topics("x").len(), 4);
        assert_eq!(event_topic("luxel-abc"), "luxel/luxel-abc/event");
        assert!(command_topics("luxel-abc").contains(&event_topic("luxel-abc")));
    }

    #[test]
    fn event_line_parsing() {
        use crate::fixed::Fx;
        let fx = |v: f64| Fx::from_f64(v);
        // full quad
        assert_eq!(
            parse_event_lines("1 0.5 0.25 0.75"),
            alloc::vec![[fx(1.0), fx(0.5), fx(0.25), fx(0.75)]]
        );
        // defaults: bare type → x/y 0, value 1; negative + int coords fine
        assert_eq!(parse_event_lines("2"), alloc::vec![[fx(2.0), fx(0.0), fx(0.0), fx(1.0)]]);
        assert_eq!(
            parse_event_lines("1 -0.5"),
            alloc::vec![[fx(1.0), fx(-0.5), fx(0.0), fx(1.0)]]
        );
        // multiple lines, blanks and junk skipped, CRLF tolerated
        let evs = parse_event_lines("1 0.1 0.2\r\n\r\nnope\n3\n");
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1][0], fx(3.0));
        // trailing garbage keeps the parsed prefix
        assert_eq!(
            parse_event_lines("1 0.5 what"),
            alloc::vec![[fx(1.0), fx(0.5), fx(0.0), fx(1.0)]]
        );
        // batch cap
        let big: String = (0..40).map(|_| "1\n").collect();
        assert_eq!(parse_event_lines(&big).len(), crate::netin::EV_MAX_BATCH);
        // empty/garbage-only payloads parse to nothing
        assert!(parse_event_lines("").is_empty());
        assert!(parse_event_lines("{\"not\":\"numbers\"}").is_empty());
    }

    #[test]
    fn fx_decimal_parsing_matches_float() {
        use crate::fixed::Fx;
        for s in ["0", "1", "-1", "0.5", "-0.25", ".75", "3.", "12.345", "-100.001", "+2.5"] {
            let want = Fx::from_f64(s.parse::<f64>().unwrap());
            let got = parse_fx(s).unwrap();
            assert!(
                (got.raw() - want.raw()).abs() <= 1,
                "{s}: got raw {} want {}",
                got.raw(),
                want.raw()
            );
        }
        for s in ["", ".", "-", "1e3", "0x10", "one", "1.2.3", "1,5"] {
            assert!(parse_fx(s).is_none(), "{s} should not parse");
        }
        // out-of-range clamps instead of wrapping
        assert_eq!(parse_fx("999999999").unwrap().raw(), i32::MAX);
        assert_eq!(parse_fx("-999999999").unwrap().raw(), -i32::MAX);
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
