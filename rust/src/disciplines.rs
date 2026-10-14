use crate::model::Dataset;

#[derive(Clone, Debug)]
pub struct Rule {
    pub keywords: &'static [&'static str],
    pub target_layer: &'static str,
    pub color_aci: u16,
    pub label: &'static str,
}

// Ordered to maintain the existing Python settings defaults, including
// the first-matching-rule precedence.
pub const RULES: &[Rule] = &[
    Rule { keywords: &["water"], target_layer: "N-OI-KL-WATER_DISTRIBUTIELEIDING_PVCBV_110-G", color_aci: 5, label: "Waterleiding" },
    Rule { keywords: &["data"], target_layer: "N-OI-KL-DATA-G", color_aci: 3, label: "Datakabel" },
    Rule { keywords: &["glasvezel","gv"], target_layer: "N-OI-KL-DATA_GLASVEZEL-G", color_aci: 100, label: "Glasvezel" },
    Rule { keywords: &["ld"], target_layer: "N-OI-KL-GAS_LD-G", color_aci: 50, label: "Gasleiding LD" },
    Rule { keywords: &["hd"], target_layer: "N-OI-KL-GAS_HD-G", color_aci: 30, label: "Gasleiding HD" },
    Rule { keywords: &["ls"], target_layer: "N-OI-KL-ET_LS-G", color_aci: 190, label: "Laagspanning" },
    Rule { keywords: &["ms"], target_layer: "N-OI-KL-ET_MS-G", color_aci: 4, label: "Middenspanning" },
    Rule { keywords: &["monitoring","x"], target_layer: "N-OI-KL-MONITORING_KABEL-G", color_aci: 1, label: "Monitoring" },
    Rule { keywords: &["rio","druk"], target_layer: "N-OI-RI-DRUK-G", color_aci: 210, label: "Riool druk" },
    Rule { keywords: &["verval","vrijverval"], target_layer: "N-OI-RI-VRIJVERVAL-G", color_aci: 210, label: "Riool vrijverval" },
    Rule { keywords: &["gevaarlijke inhoud","gs"], target_layer: "N-OI-KL-BUISLEIDING GEVAARLIJKE INHOUD-G", color_aci: 20, label: "Buisleiding GS" },
];

#[derive(Clone, Debug)]
pub struct Summary {
    pub proefsleuf: String,
    pub counts: Vec<u32>,
}

impl Summary {
    pub fn distinct_count(&self) -> usize {
        self.counts.iter().filter(|&&v| v > 0).count()
    }
    pub fn total_count(&self) -> u32 {
        self.counts.iter().sum()
    }
}

/// Glob matching for the original Python fnmatch "*" and "?" keywords.
fn glob_matches(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let (mut star, mut match_pos) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            match_pos = t;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            match_pos += 1;
            t = match_pos;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' { p += 1; }
    p == pattern.len()
}

pub fn matching_rule<'a>(name: &str, rules: &'a [Rule]) -> Option<&'a Rule> {
    let name = name.trim().to_lowercase();
    if name.is_empty() { return None; }
    rules.iter().find(|rule| rule.keywords.iter().any(|word| {
        let keyword = word.trim().to_lowercase();
        if keyword.is_empty() { return false; }
        if (keyword.contains('*') || keyword.contains('?')) {
            glob_matches(keyword.as_bytes(), name.as_bytes())
        } else { name.contains(&keyword) }
    }))
}

pub fn summarize(dataset: &Dataset, rules: &[Rule]) -> Summary {
    let mut counts = vec![0u32; rules.len()];
    for feature in &dataset.features {
        if let Some(index) = rules.iter().position(|r| {
            matching_rule(&feature.source_name, std::slice::from_ref(r)).is_some()
        }) {
            counts[index] = counts[index].saturating_add(1);
        }
    }
    Summary {
        proefsleuf: dataset.title.clone(), counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Dataset, Feature, Geometry, Vertex};
    use std::path::PathBuf;

    #[test]
    fn first_match_and_polyline_once() {
        let names = ["Water", "Water PVC", "Data", "LS", "niets"];
        let features = names.iter().map(|name| Feature {
            object_name: (*name).into(),
            source_name: (*name).into(),
            geometry: Geometry::Point(Vertex {x:1.,y:2.,z:None}),
            attributes: Default::default(),
        }).collect();
        let ds = Dataset { job_id:1, title:"PS12".into(), source_path:PathBuf::new(), features };
        let result = summarize(&ds, RULES);
        assert_eq!(result.distinct_count(), 3);
        assert_eq!(result.total_count(), 4);
        assert_eq!(result.counts[0], 2);
    }

    #[test]
    fn glob_and_case() {
        let custom = [Rule {keywords:&["*FIB?R*"],target_layer:"DATA",color_aci:3,label:"Fiber"}];
        assert!(matching_rule("fiber cable", &custom).is_some());
        assert!(matching_rule("water", &custom).is_none());
    }
}