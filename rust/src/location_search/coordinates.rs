use super::Location;
use proj4rs::proj::Proj;
use std::sync::OnceLock;

fn numbers(query: &str) -> Vec<f64> {
    // Python 3.11's Unicode-14 decimal categories, including mixed alphabets.
    let mut chars = query.chars().peekable();
    let mut output = Vec::new();
    while let Some(ch) = chars.next() {
        let mut number = String::new();
        if ch == '-'
            && chars
                .peek()
                .is_some_and(|c| crate::object_layout::decimal(*c).is_some())
        {
            number.push('-');
        } else if let Some(n) = crate::object_layout::decimal(ch) {
            number.push(char::from(b'0' + n as u8));
        } else {
            continue;
        }
        let mut dot = false;
        while let Some(&c) = chars.peek() {
            if let Some(n) = crate::object_layout::decimal(c) {
                number.push(char::from(b'0' + n as u8));
                chars.next();
            } else if !dot && matches!(c, '.' | ',') {
                let mut look = chars.clone();
                look.next();
                if look
                    .peek()
                    .is_none_or(|c| crate::object_layout::decimal(*c).is_none())
                {
                    break;
                }
                dot = true;
                number.push('.');
                chars.next();
            } else {
                break;
            }
        }
        if let Ok(value) = number.parse() {
            output.push(value);
        }
    }
    output
}
pub fn parse_rd(query: &str) -> Option<[f64; 2]> {
    if query.len() > 4096 {
        return None;
    }
    let values = numbers(query);
    (values.len() == 2 && values.iter().all(|v| v.is_finite() && *v >= 1000.))
        .then(|| [values[0], values[1]])
}
/// The same explicit 7-parameter operation selected by the original pyproj
/// EPSG:4326→28992 transformer without downloaded datum grids. Navigation only;
/// source survey coordinates and elevations are never modified.
pub fn wgs84_to_rd(lat: f64, lon: f64) -> Result<[f64; 2], String> {
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90. ..=90.).contains(&lat)
        || !(-180. ..=180.).contains(&lon)
    {
        return Err("Ongeldige GPS-coördinaten".into());
    }
    static PROJECTIONS: OnceLock<Result<(Proj, Proj), String>> = OnceLock::new();
    let (from,to)=PROJECTIONS.get_or_init(||{
        let from=Proj::from_proj_string("+proj=longlat +ellps=WGS84 +datum=WGS84 +no_defs").map_err(|e|e.to_string())?;
        let to=Proj::from_proj_string(crate::rd_projection::RD).map_err(|e|e.to_string())?;
        Ok((from,to))
    }).as_ref().map_err(Clone::clone)?;
    let mut point = (lon.to_radians(), lat.to_radians(), 0.);
    proj4rs::transform::transform(from, to, &mut point).map_err(|e| e.to_string())?;
    if !point.0.is_finite() || !point.1.is_finite() {
        return Err("GPS-locatie kon niet naar RD worden omgerekend".into());
    }
    Ok([point.0, point.1])
}
pub fn parse_coordinate(query: &str) -> Option<Location> {
    if query.len() > 4096 {
        return None;
    }
    let query = crate::python_values::trim(query);
    if let Some([x, y]) = parse_rd(query) {
        return Some(Location {
            label: format!("RD locatie: X {x:.2}, Y {y:.2}"),
            x,
            y,
            location_type: "rd".into(),
        });
    }
    static PREFIX: OnceLock<regex::Regex> = OnceLock::new();
    let value = PREFIX
        .get_or_init(|| regex::Regex::new(r"(?i)\bwgs\s*84\b|\bepsg\s*:?\s*4326\b").unwrap())
        .replace_all(query, " ");
    let values = numbers(&value);
    let (&first, &second) = (values.first()?, values.get(1)?);
    let lower = crate::python_values::casefold(query);
    let explicit = ["wgs", "gps", "lat", "lon", "lng"]
        .iter()
        .any(|s| lower.contains(s));
    static AXES: OnceLock<regex::Regex> = OnceLock::new();
    let mut lat_pos = None;
    let mut lon_pos = None;
    for name in AXES
        .get_or_init(|| regex::Regex::new(r"\b(lat(?:itude)?|lon(?:gitude)?|lng)\b").unwrap())
        .find_iter(&lower)
    {
        if name.as_str().starts_with("lat") {
            lat_pos.get_or_insert(name.start());
        } else {
            lon_pos.get_or_insert(name.start());
        }
    }
    let (lat, lon) = if let (Some(lat_pos), Some(lon_pos)) = (lat_pos, lon_pos) {
        if lat_pos < lon_pos {
            (first, second)
        } else {
            (second, first)
        }
    } else if (50. ..=54.5).contains(&first) && (2.5..=8.).contains(&second) {
        (first, second)
    } else if (2.5..=8.).contains(&first) && (50. ..=54.5).contains(&second) {
        // The original prefix branch misread "GPS 5.2 52.1" as latitude 5.2.
        // Infer the unambiguous Dutch order before accepting the global fallback.
        (second, first)
    } else if (-90. ..=90.).contains(&first) && (-180. ..=180.).contains(&second) && explicit {
        (first, second)
    } else if (-180. ..=180.).contains(&first) && (-90. ..=90.).contains(&second) && explicit {
        (second, first)
    } else {
        return None;
    };
    let [x, y] = wgs84_to_rd(lat, lon).ok()?;
    Some(Location {
        label: format!("GPS locatie: {lat:.6}, {lon:.6}"),
        x,
        y,
        location_type: "wgs84".into(),
    })
}
