pub fn expression(condition: u8) -> Result<&'static str, String> {
    Ok(match condition {
        0 => "c->z",
        1 => "!c->z",
        2 => "c->c",
        3 => "!c->c",
        4 => "c->n",
        5 => "!c->n",
        6 => "c->v",
        7 => "!c->v",
        8 => "c->c&&!c->z",
        9 => "!c->c||c->z",
        10 => "c->n==c->v",
        11 => "c->n!=c->v",
        12 => "!c->z&&c->n==c->v",
        13 => "c->z||c->n!=c->v",
        14 | 15 => "1",
        _ => return Err("invalid condition".into()),
    })
}
