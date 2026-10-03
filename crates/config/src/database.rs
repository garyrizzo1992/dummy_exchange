//! Connection configuration shared by services and the migration command.

use anyhow::Context;
use sqlx::postgres::PgConnectOptions;
use std::env;

pub fn connection_options() -> anyhow::Result<PgConnectOptions> {
    options_from_lookup(|key| env::var(key))
}

fn options_from_lookup(
    mut lookup: impl FnMut(&str) -> Result<String, env::VarError>,
) -> anyhow::Result<PgConnectOptions> {
    match lookup("DATABASE_URL") {
        Ok(url) => url.parse().context("invalid DATABASE_URL"),
        Err(env::VarError::NotPresent) => {
            // Pass credentials directly to SQLx: punctuation in passwords is not
            // interpreted as URL syntax. Preserve DATABASE_URL for Compose users.
            let host = lookup("PGHOST").context("PGHOST or DATABASE_URL is required")?;
            let user = lookup("PGUSER").context("PGUSER is required")?;
            let database = lookup("PGDATABASE").context("PGDATABASE is required")?;
            let password = lookup("PGPASSWORD").context("PGPASSWORD is required")?;
            let port = optional_value(&mut lookup, "PGPORT", "5432")?;
            let ssl_mode = optional_value(&mut lookup, "PGSSLMODE", "prefer")?;
            Ok(PgConnectOptions::new()
                .host(&host)
                .port(port.parse().context("invalid PGPORT")?)
                .username(&user)
                .database(&database)
                .ssl_mode(ssl_mode.parse().context("invalid PGSSLMODE")?)
                .password(&password))
        }
        Err(error) => Err(error).context("DATABASE_URL must contain valid Unicode"),
    }
}

fn optional_value(
    lookup: &mut impl FnMut(&str) -> Result<String, env::VarError>,
    key: &str,
    default: &str,
) -> anyhow::Result<String> {
    match lookup(key) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default.into()),
        Err(error) => Err(error).with_context(|| format!("invalid {key}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{ConnectOptions, postgres::PgSslMode};

    fn options(values: &[(&str, &str)]) -> anyhow::Result<PgConnectOptions> {
        options_from_lookup(|key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).into())
                .ok_or(env::VarError::NotPresent)
        })
    }

    #[test]
    fn separate_credentials_preserve_password_punctuation() {
        let connection = options(&[
            ("PGHOST", "database.internal"),
            ("PGUSER", "exchange"),
            ("PGDATABASE", "exchange"),
            ("PGPASSWORD", "a@b:c/d?#%"),
            ("PGPORT", "5433"),
            ("PGSSLMODE", "require"),
        ])
        .unwrap();
        assert_eq!(connection.get_host(), "database.internal");
        assert_eq!(connection.get_port(), 5433);
        assert!(matches!(connection.get_ssl_mode(), PgSslMode::Require));
        // Serializing and parsing again must preserve the encoded password.
        let url = connection.to_url_lossy();
        let reparsed: PgConnectOptions = url.as_str().parse().unwrap();
        assert_eq!(reparsed.to_url_lossy(), url);
        assert_eq!(url.password(), Some("a%40b%3Ac%2Fd%3F%23%25"));
    }

    #[test]
    fn compose_database_url_takes_precedence() {
        let connection = options(&[
            ("DATABASE_URL", "postgres://demo:password@compose-db/demo"),
            ("PGHOST", "other-db"),
        ])
        .unwrap();
        assert_eq!(connection.get_host(), "compose-db");
        assert_eq!(connection.get_username(), "demo");
        assert_eq!(connection.get_database(), Some("demo"));
    }

    #[test]
    fn missing_configuration_fails_instead_of_using_localhost() {
        assert!(options(&[]).unwrap_err().to_string().contains("PGHOST"));
    }
}
