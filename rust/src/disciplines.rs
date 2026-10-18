use crate::model::Dataset;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub keywords: Cow<'static, [Cow<'static, str>]>,
    pub target_layer: Cow<'static, str>,
    pub color_aci: u16,
    pub label: Cow<'static, str>,
}
impl Rule {
    pub fn new(keywords: Vec<String>, target_layer: String, color_aci: u16, label: String) -> Self {
        Self {
            keywords: Cow::Owned(keywords.into_iter().map(Cow::Owned).collect()),
            target_layer: Cow::Owned(target_layer),
            color_aci,
            label: Cow::Owned(label),
        }
    }
    pub fn display_label(&self) -> &str {
        let label = crate::python_values::trim(&self.label);
        if !label.is_empty() {
            return label;
        }
        if let Some(first) = self.keywords.first() {
            let first = crate::python_values::trim(first);
            if !first.is_empty() {
                return first;
            }
        }
        let layer = crate::python_values::trim(&self.target_layer);
        if layer.is_empty() {
            "Onbekend"
        } else {
            layer
        }
    }
}

// Ordered to maintain the existing Python settings defaults, including
// the first-matching-rule precedence.
pub const RULES: &[Rule] = &[
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("water")]),
        target_layer: Cow::Borrowed("N-OI-KL-WATER_DISTRIBUTIELEIDING_PVCBV_110-G"),
        color_aci: 5,
        label: Cow::Borrowed("Waterleiding"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("data")]),
        target_layer: Cow::Borrowed("N-OI-KL-DATA-G"),
        color_aci: 3,
        label: Cow::Borrowed("Datakabel"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("glasvezel"), Cow::Borrowed("gv")]),
        target_layer: Cow::Borrowed("N-OI-KL-DATA_GLASVEZEL-G"),
        color_aci: 100,
        label: Cow::Borrowed("Glasvezel"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("ld")]),
        target_layer: Cow::Borrowed("N-OI-KL-GAS_LD-G"),
        color_aci: 50,
        label: Cow::Borrowed("Gasleiding LD"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("hd")]),
        target_layer: Cow::Borrowed("N-OI-KL-GAS_HD-G"),
        color_aci: 30,
        label: Cow::Borrowed("Gasleiding HD"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("ls")]),
        target_layer: Cow::Borrowed("N-OI-KL-ET_LS-G"),
        color_aci: 190,
        label: Cow::Borrowed("Laagspanning"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("ms")]),
        target_layer: Cow::Borrowed("N-OI-KL-ET_MS-G"),
        color_aci: 4,
        label: Cow::Borrowed("Middenspanning"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("monitoring"), Cow::Borrowed("x")]),
        target_layer: Cow::Borrowed("N-OI-KL-MONITORING_KABEL-G"),
        color_aci: 1,
        label: Cow::Borrowed("Monitoring"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("rio"), Cow::Borrowed("druk")]),
        target_layer: Cow::Borrowed("N-OI-RI-DRUK-G"),
        color_aci: 210,
        label: Cow::Borrowed("Riool druk"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("verval"), Cow::Borrowed("vrijverval")]),
        target_layer: Cow::Borrowed("N-OI-RI-VRIJVERVAL-G"),
        color_aci: 210,
        label: Cow::Borrowed("Riool vrijverval"),
    },
    Rule {
        keywords: Cow::Borrowed(&[Cow::Borrowed("gevaarlijke inhoud"), Cow::Borrowed("gs")]),
        target_layer: Cow::Borrowed("N-OI-KL-BUISLEIDING GEVAARLIJKE INHOUD-G"),
        color_aci: 20,
        label: Cow::Borrowed("Buisleiding GS"),
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
        crate::rule_glob::matches(
            keyword,
            uppercase.get_or_init(|| crate::python_values::uppercase(name)),
        )
    } else if keyword.is_ascii() {
        let source = if name.is_ascii() {
            name
        } else {
            uppercase.get_or_init(|| crate::python_values::uppercase(name))
        };
        source
            .as_bytes()
            .windows(keyword.len())
            .any(|part| part.eq_ignore_ascii_case(keyword.as_bytes()))
    } else {
        uppercase
            .get_or_init(|| crate::python_values::uppercase(name))
            .contains(&crate::python_values::uppercase(keyword))
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
    summarize_grouped(dataset, rules, &rule_groups(rules))
}
/// Canonical rule indices, retaining the first spelling and order of labels.
pub(crate) fn rule_groups(rules: &[Rule]) -> Vec<usize> {
    let mut seen = std::collections::HashMap::new();
    rules
        .iter()
        .enumerate()
        .map(|(i, r)| {
            *seen
                .entry(crate::python_values::casefold(r.display_label()))
                .or_insert(i)
        })
        .collect()
}
pub(crate) fn summarize_grouped(dataset: &Dataset, rules: &[Rule], groups: &[usize]) -> Summary {
    summarize_grouped_cancel(dataset, rules, groups, &crate::input::NEVER_CANCEL)
        .expect("uncancelled summary")
}
pub(crate) fn summarize_grouped_cancel(
    dataset: &Dataset,
    rules: &[Rule],
    groups: &[usize],
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Summary, String> {
    let mut counts = vec![0u32; rules.len()];
    for (index, feature) in dataset.features.iter().enumerate() {
        if index % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if let Some(index) = matching_index(&feature.source_name, rules) {
            let column = groups[index];
            counts[column] = counts[column].saturating_add(1);
        }
    }
    Ok(Summary {
        proefsleuf: crate::python_values::trim(&dataset.title).into(),
        counts,
    })
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
            keywords: Cow::Borrowed(&[Cow::Borrowed("*FIB?R*")]),
            target_layer: Cow::Borrowed("DATA"),
            color_aci: 3,
            label: Cow::Borrowed("Fiber"),
        }];
        assert!(matching_rule("fiber cable", &custom).is_some());
        assert!(matching_rule("water", &custom).is_none());
    }
}
