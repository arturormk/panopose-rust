#[derive(Debug, Clone)]
pub struct StellariumLandscape {
    pub name: String,
    pub author: String,
    pub description: String,
    pub maptex: String,
    pub angle_rotatez_deg: f64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: f64,
}

pub fn stellarium_landscape_ini(landscape: &StellariumLandscape) -> String {
    format!(
        "[landscape]\n\
         name = {}\n\
         author = {}\n\
         description = {}\n\
         type = spherical\n\
         maptex = {}\n\
         angle_rotatez = {}\n\
         \n\
         [location]\n\
         planet = Earth\n\
         latitude = {}\n\
         longitude = {}\n\
         altitude = {}\n",
        sanitize_ini_value(&landscape.name),
        sanitize_ini_value(&landscape.author),
        sanitize_ini_value(&landscape.description),
        sanitize_ini_value(&landscape.maptex),
        format_decimal(landscape.angle_rotatez_deg),
        format_dms(landscape.latitude_deg),
        format_dms(landscape.longitude_deg),
        landscape.altitude_m.round() as i64,
    )
}

fn sanitize_ini_value(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn format_decimal(value: f64) -> String {
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    if rounded.fract().abs() < 1e-9 {
        format!("{}", rounded as i64)
    } else {
        let formatted = format!("{rounded:.6}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn format_dms(value: f64) -> String {
    let sign = if value < 0.0 { '-' } else { '+' };
    let total_seconds = (value.abs() * 3600.0).round() as i64;
    let degrees = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{sign}{degrees:02}d{minutes:02}'{seconds:02}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_landscape_ini() {
        let ini = stellarium_landscape_ini(&StellariumLandscape {
            name: "East Terrace".to_string(),
            author: "Arturo R Montesinos".to_string(),
            description: "Santa Maria 5, Pelayos de la Presa".to_string(),
            maptex: "pelayos-east-terrace.png".to_string(),
            angle_rotatez_deg: -90.0,
            latitude_deg: 40.364444,
            longitude_deg: -4.319722,
            altitude_m: 602.2,
        });

        assert!(ini.contains("type = spherical\n"));
        assert!(ini.contains("maptex = pelayos-east-terrace.png\n"));
        assert!(ini.contains("angle_rotatez = -90\n"));
        assert!(ini.contains("latitude = +40d21'52\"\n"));
        assert!(ini.contains("longitude = -04d19'11\"\n"));
        assert!(ini.contains("altitude = 602\n"));
    }

    #[test]
    fn removes_newlines_from_ini_values() {
        let ini = stellarium_landscape_ini(&StellariumLandscape {
            name: "A\nB".to_string(),
            author: "C\tD".to_string(),
            description: "E\r\nF".to_string(),
            maptex: "map.png".to_string(),
            angle_rotatez_deg: -89.5,
            latitude_deg: 0.0,
            longitude_deg: 0.0,
            altitude_m: 0.0,
        });

        assert!(ini.contains("name = A B\n"));
        assert!(ini.contains("author = C D\n"));
        assert!(ini.contains("description = E F\n"));
        assert!(ini.contains("angle_rotatez = -89.5\n"));
    }
}
