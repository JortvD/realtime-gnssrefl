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
            max_elevation: 15.0,
            min_azimuth: 50.0,
            max_azimuth: 150.0,
            min_height: 2.0,
            max_height: 7.0,
            step_size: 0.01,
        }
    }
}