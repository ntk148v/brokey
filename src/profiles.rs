use crate::audio::Clip;
use crate::audio::decode;

#[derive(Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub press: Clip,
    pub space: Option<Clip>,
}

pub fn builtins() -> Vec<Profile> {
    let press_bytes = include_bytes!("../assets/sounds/mx-blue/press.wav").to_vec();
    let space_bytes = include_bytes!("../assets/sounds/mx-blue/space.wav").to_vec();
    let press = decode(press_bytes).expect("built-in press.wav must decode");
    let space = decode(space_bytes).expect("built-in space.wav must decode");
    vec![Profile {
        id: "mx-blue".to_string(),
        name: "Cherry MX Blue".to_string(),
        press,
        space: Some(space),
    }]
}
