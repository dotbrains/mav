#[path = "docker_in_docker_install_part1.rs"]
mod part1;
#[path = "docker_in_docker_install_part2.rs"]
mod part2;
#[path = "docker_in_docker_install_part3.rs"]
mod part3;

pub(crate) const INSTALL_SH: &str = concat!(part1::PART, part2::PART, part3::PART);
