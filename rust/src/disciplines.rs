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
    Rule {
        keywords: &["water"],
        target_layer: "N-OI-KL-WATER_DISTRIBUTIELEIDING_PVCBV_110-G",
        color_aci: 5,
        label: "Waterleiding",
    },
    Rule {
        keywords: &["data"],
        target_layer: "N-OI-KL-DATA-G",
        color_aci: 3,
        label: "Datakabel",
    },
    Rule {
        keywords: &["glasvezel", "gv"],
        target_layer: "N-OI-KL-DATA_GLASVEZEL-G",
        color_aci: 100,
        label: "Glasvezel",
    },
    Rule {
        keywords: &["ld"],
        target_layer: "N-OI-KL-GAS_LD-G",
        color_aci: 50,
        label: "Gasleiding LD",
    },
    Rule {
        keywords: &["hd"],
        target_layer: "N-OI-KL-GAS_HD-G",
        color_aci: 30,
        label: "Gasleiding HD",
    },
    Rule {
        keywords: &["ls"],
        target_layer: "N-OI-KL-ET_LS-G",
        color_aci: 190,
        label: "Laagspanning",
    },
    Rule {
        keywords: &["ms"],
        target_layer: "N-OI-KL-ET_MS-G",
        color_aci: 4,
        label: "Middenspanning",
    },
    Rule {
        keywords: &["monitoring", "x"],
        target_layer: "N-OI-KL-MONITORING_KABEL-G",
        color_aci: 1,
        label: "Monitoring",
    },
    Rule {
        keywords: &["rio", "druk"],
        target_layer: "N-OI-RI-DRUK-G",
        color_aci: 210,
        label: "Riool druk",
    },
    Rule {
        keywords: &["verval", "vrijverval"],
        target_layer: "N-OI-RI-VRIJVERVAL-G",
        color_aci: 210,
        label: "Riool vrijverval",
    },
    Rule {
        keywords: &["gevaarlijke inhoud", "gs"],
        target_layer: "N-OI-KL-BUISLEIDING GEVAARLIJKE INHOUD-G",
        color_aci: 20,
        label: "Buisleiding GS",
    },
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

fn keyword_matches(name: &str, keyword: &str, uppercase: &std::cell::OnceCell<String>) -> bool {
    let keyword = crate::python_values::trim(keyword);
    if keyword.is_empty() {
        return false;
    }
    if keyword.contains(['*', '?']) {
        crate::rule_glob::matches(keyword, uppercase.get_or_init(|| name.to_uppercase()))
    } else if keyword.is_ascii() {
        let source = if name.is_ascii() {
            name
        } else {
            uppercase.get_or_init(|| name.to_uppercase())
        };
        source
            .as_bytes()
            .windows(keyword.len())
            .any(|part| part.eq_ignore_ascii_case(keyword.as_bytes()))
    } else {
        uppercase
            .get_or_init(|| name.to_uppercase())
            .contains(&keyword.to_uppercase())
    }
}
/// One original ObjectLayerRule keyword; brackets are patterns only when the
/// keyword also contains * or ?, exactly as in the Python selection logic.
pub fn matches_keyword(name: &str, keyword: &str) -> bool {
    let name = crate::python_values::trim(name);
    !name.is_empty() && keyword_matches(name, keyword, &std::cell::OnceCell::new())
}
fn matching_index(name: &str, rules: &[Rule]) -> Option<usize> {
    let name = crate::python_values::trim(name);
    if name.is_empty() {
        return None;
    }
    let uppercase = std::cell::OnceCell::new();
    rules.iter().position(|rule| {
        rule.keywords
            .iter()
            .any(|word| keyword_matches(name, word, &uppercase))
    })
}
pub fn matching_rule<'a>(name: &str, rules: &'a [Rule]) -> Option<&'a Rule> {
    matching_index(name, rules).map(|i| &rules[i])
}

pub fn summarize(dataset: &Dataset, rules: &[Rule]) -> Summary {
    let mut counts = vec![0u32; rules.len()];
    for feature in &dataset.features {
        if let Some(index) = matching_index(&feature.source_name, rules) {
            counts[index] = counts[index].saturating_add(1);
        }
    }
    Summary {
        proefsleuf: dataset.title.clone(),
        counts,
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
        let features = names
            .iter()
            .map(|name| Feature {
                object_name: (*name).into(),
                source_name: (*name).into(),
                geometry: Geometry::Point(Vertex {
                    x: 1.,
                    y: 2.,
                    z: None,
                }),
                attributes: Default::default(),
            })
            .collect();
        let ds = Dataset {
            job_id: 1,
            title: "PS12".into(),
            source_path: PathBuf::new(),
            features,
        };
        let result = summarize(&ds, RULES);
        assert_eq!(result.distinct_count(), 3);
        assert_eq!(result.total_count(), 4);
        assert_eq!(result.counts[0], 2);
    }

    #[test]
    fn glob_and_case() {
        let custom = [Rule {
            keywords: &["*FIB?R*"],
            target_layer: "DATA",
            color_aci: 3,
            label: "Fiber",
        }];
        assert!(matching_rule("fiber cable", &custom).is_some());
        assert!(matching_rule("water", &custom).is_none());
    }
}
