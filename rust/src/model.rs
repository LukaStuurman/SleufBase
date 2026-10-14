use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Geometry {
    Point(Vertex),
    Polyline(Vec<Vertex>),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Feature {
    pub object_name: String,
    pub source_name: String,
    pub geometry: Geometry,
    pub attributes: [String; 3],
}

#[derive(Clone, Debug)]
pub struct Dataset {
    pub job_id: u64,
    pub title: String,
    pub source_path: PathBuf,
    pub features: Vec<Feature>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    pub fn from_datasets(datasets: &[Dataset]) -> Option<Self> {
        let mut bounds: Option<Bounds> = None;
        for dataset in datasets {
            for feature in &dataset.features {
                let mut add = |v: &Vertex| {
                    if !v.x.is_finite() || !v.y.is_finite() { return; }
                    if let Some(b) = bounds.as_mut() {
                        b.min_x = b.min_x.min(v.x);
                        b.min_y = b.min_y.min(v.y);
                        b.max_x = b.max_x.max(v.x);
                        b.max_y = b.max_y.max(v.y);
                    } else {
                        bounds = Some(Bounds {
                            min_x: v.x, min_y: v.y, max_x: v.x, max_y: v.y
                        });
                    }
                };
                match &feature.geometry {
                    Geometry::Point(point) => add(point),
                    Geometry::Polyline(points) => {
                        for point in points { add(point); }
                    }
                }
            }
        }
        bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_bounds() {
        assert_eq!(Bounds::from_datasets(&[]), None);
    }
}