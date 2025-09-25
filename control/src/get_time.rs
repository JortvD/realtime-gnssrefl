use crate::gnss::GNSSSensor;

pub async fn get_time(gnss_sensor: &mut GNSSSensor) -> u32 {
    let nmeaburst = gnss_sensor.read_burst().await;
    let burst = gnss_sensor.parser.parse_burst(&nmeaburst);
    burst.time
}