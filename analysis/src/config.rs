pub struct Config {
    pub min_elevation: f64,
    pub max_elevation: f64,
    pub min_azimuth: f64,
    pub max_azimuth: f64,
    pub min_height: f64,
    pub max_height: f64,
    pub step_size: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            min_elevation: 5.0,
            max_elevation: 90.0,
            min_azimuth: 0.0,
            max_azimuth: 360.0,
            min_height: 1.0,
            max_height: 8.0,
            step_size: 0.01,
        }
    }
}