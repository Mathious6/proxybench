use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Alive,
    Partial,
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Health {
    pub status: Status,
    pub ok: usize,
    pub tested: usize,
    pub at: u64,
}

impl Health {
    pub fn new(ok: usize, tested: usize, at: u64) -> Self {
        Self {
            status: status_of(ok, tested),
            ok,
            tested,
            at,
        }
    }
}

fn status_of(ok: usize, tested: usize) -> Status {
    if ok == 0 {
        Status::Dead
    } else if ok * 5 >= tested * 4 {
        Status::Alive
    } else {
        Status::Partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eighty_percent_success_is_alive() {
        assert_eq!(Health::new(4, 5, 0).status, Status::Alive);
        assert_eq!(Health::new(10, 10, 0).status, Status::Alive);
        assert_eq!(Health::new(8, 10, 0).status, Status::Alive);
    }

    #[test]
    fn below_eighty_percent_with_some_success_is_partial() {
        assert_eq!(Health::new(3, 5, 0).status, Status::Partial);
        assert_eq!(Health::new(1, 2, 0).status, Status::Partial);
        assert_eq!(Health::new(79, 100, 0).status, Status::Partial);
    }

    #[test]
    fn no_success_is_dead() {
        assert_eq!(Health::new(0, 5, 0).status, Status::Dead);
        assert_eq!(Health::new(0, 0, 0).status, Status::Dead);
    }

    #[test]
    fn serializes_status_lowercase_beside_counts() {
        let json = serde_json::to_value(Health::new(1, 2, 7)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"status": "partial", "ok": 1, "tested": 2, "at": 7})
        );
    }
}
