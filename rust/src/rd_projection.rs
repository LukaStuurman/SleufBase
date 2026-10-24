//! The grid-free seven-parameter RD operation also used by the Python reference.
pub(crate) const RD: &str = concat!(
    "+proj=sterea +lat_0=52.1561605555556 +lon_0=5.38763888888889 +k=0.9999079 ",
    "+x_0=155000 +y_0=463000 +ellps=bessel +units=m ",
    "+towgs84=565.4171,50.3319,465.5524,-0.398957388243134,0.343987817378283,-1.87740163998045,4.0725 +no_defs"
);
