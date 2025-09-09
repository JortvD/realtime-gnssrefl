pub struct Config {
    pub min_elevation: f64,
    pub max_elevation: f64,
    pub min_azimuth: f64,
    pub max_azimuth: f64,
    pub min_height: f64,
    pub max_height: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            min_elevation: 5.0,
            max_elevation: 30.0,
            min_azimuth: 240.0,
            max_azimuth: 320.0,
            min_height: 5.0,
            max_height: 18.0,
        }
    }
}