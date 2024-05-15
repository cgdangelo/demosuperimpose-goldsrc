extern crate demosuperimpose_goldsrc;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::identity;
use std::env::args;
use std::fs::File;
use std::io::Read;
use std::str::from_utf8;

use nom::IResult;
use nom::number::complete::{be_u16, le_u8};
use nom::sequence::tuple;

use demosuperimpose_goldsrc::netmsg_doer::parse_netmsg;
use demosuperimpose_goldsrc::netmsg_doer::utils::get_initial_delta;
use demosuperimpose_goldsrc::open_demo;
use demosuperimpose_goldsrc::types::{EngineMessage, Message, SvcNewUserMsg, SvcUpdateUserInfo};

#[derive(Debug)]
struct Frags {
    player_idx: u8,
    value: u8,
}

#[derive(Debug)]
struct ScoreShort {
    player_idx: u8,
    points: u8,
    kills: u16,
    deaths: u16,
}

#[derive(Debug)]
struct ScoreInfo {
    player_idx: u8,
    points: u8,
    kills: u8,
    deaths: u8,
    class: u8,
    team: u8,
}

#[derive(Clone, Debug)]
enum Team {
    ALLIES,
    AXIS,
    SPECTATORS,
    UNKNOWN,
}

impl From<u8> for Team {
    fn from(value: u8) -> Self {
        match value {
            1 => Team::ALLIES,
            2 => Team::AXIS,
            3 => Team::SPECTATORS,
            _ => Team::UNKNOWN,
        }
    }
}

impl From<&str> for Team {
    fn from(value: &str) -> Self {
        match value {
            "allies" => Team::ALLIES,
            "axis" => Team::AXIS,
            "spectators" => Team::SPECTATORS,
            _ => Team::UNKNOWN,
        }
    }
}

#[derive(Debug)]
struct ScoreboardEntry {
    name: String,
    team: Team,
    points: u8,
    kills: u8,
    deaths: u8,
}

fn get_update_user_info_fields<'a>(
    update_user_info: &'a SvcUpdateUserInfo,
) -> Option<HashMap<&'a str, &'a str>> {
    let mut info_fields: HashMap<&str, &str> = HashMap::new();

    from_utf8(update_user_info.user_info)
        .map(|s| s.trim_matches(['\0', '\\']))
        .map(|s| s.split("\\").collect::<Vec<&str>>())
        .ok()?
        .chunks_exact(2)
        .for_each(|chunk| match chunk {
            [k, v] => {
                info_fields.insert(k, v);
            }
            _ => {}
        });

    Some(info_fields)
}

fn parse_score_info(i: &[u8]) -> IResult<&[u8], ScoreInfo> {
    let (i, (player_idx, points, kills, deaths, class, team)) =
        tuple((le_u8, le_u8, le_u8, le_u8, le_u8, le_u8))(i)?;

    Ok((
        i,
        ScoreInfo {
            player_idx,
            points,
            kills,
            deaths,
            class,
            team,
        },
    ))
}

fn parse_frags(i: &[u8]) -> IResult<&[u8], Frags> {
    let (i, (player_idx, value)) = tuple((le_u8, le_u8))(i)?;

    Ok((i, Frags { player_idx, value }))
}

fn parse_score_short(i: &[u8]) -> IResult<&[u8], ScoreShort> {
    let (i, (player_idx, points, kills, deaths)) = tuple((le_u8, le_u8, be_u16, be_u16))(i)?;

    Ok((
        i,
        ScoreShort {
            player_idx,
            points,
            kills,
            deaths,
        },
    ))
}

fn main() {
    let args: Vec<String> = args().collect();

    let demo_path = match args.len() {
        2 => args.get(1).unwrap(),
        _ => panic!("Need a demo path. Re-run with: ./dod-stats ./path/to/demo.dem"),
    };

    let demo = open_demo!(demo_path);

    let mut delta_decoders = get_initial_delta();
    let mut custom_messages = HashMap::<u8, SvcNewUserMsg>::new();

    let frames = demo
        .directory
        .entries
        .iter()
        .flat_map(|directory_entry| &directory_entry.frames);

    let network_messages = frames
        .filter_map(|frame| match &frame.data {
            hldemo::FrameData::NetMsg((_, net_msg_data)) => {
                match parse_netmsg(net_msg_data.msg, &mut delta_decoders, &mut custom_messages) {
                    Ok((_, messages)) => Some(messages),
                    _ => None,
                }
            }
            _ => None,
        })
        .flat_map(identity);

    // Mapping of player index to player id.
    let mut player_id_cache: HashMap<u8, u32> = HashMap::new();

    // Mapping of player id to aggregated metrics.
    let mut scoreboard: HashMap<u32, ScoreboardEntry> = HashMap::new();

    network_messages.for_each(|message| match message {
        Message::EngineMessage(EngineMessage::SvcUpdateUserInfo(update_user_info)) => {
            let fields = get_update_user_info_fields(&update_user_info).unwrap_or(HashMap::new());

            player_id_cache.insert(update_user_info.index, update_user_info.id);

            let name = &fields
                .get("name")
                .map(|value| value.to_string())
                .unwrap_or(format!("Player {}", update_user_info.id));

            let team = &fields
                .get("team")
                .map(|value| Team::from(*value))
                .unwrap_or(Team::UNKNOWN);

            scoreboard
                .entry(update_user_info.id)
                .and_modify(|entry| {
                    entry.name = name.to_owned();
                    entry.team = team.to_owned();
                })
                .or_insert(ScoreboardEntry {
                    name: name.to_owned(),
                    team: team.to_owned(),
                    points: 0,
                    kills: 0,
                    deaths: 0,
                });
        }
        Message::UserMessage(user_msg) => {
            match from_utf8(user_msg.name).map(|s| s.trim_matches('\0')) {
                Ok("Frags") => {
                    if let Ok((_, frags)) = parse_frags(user_msg.data) {
                        let player_idx = frags.player_idx - 1;

                        if let Some(player_id) = player_id_cache.get(&player_idx) {
                            scoreboard
                                .entry(*player_id)
                                .and_modify(|entry| {
                                    entry.kills = frags.value;
                                })
                                .or_insert(ScoreboardEntry {
                                    name: format!("Player {}", player_id),
                                    points: 0,
                                    deaths: 0,
                                    kills: frags.value,
                                    team: Team::UNKNOWN,
                                });
                        }
                    }
                }

                Ok("ScoreInfo") => {
                    if let Ok((_, score_info)) = parse_score_info(user_msg.data) {
                        let player_idx = score_info.player_idx - 1;

                        if let Some(player_id) = player_id_cache.get(&player_idx) {
                            scoreboard.entry(*player_id).and_modify(|entry| {
                                entry.team = Team::from(score_info.team);
                                entry.points = score_info.points;
                                entry.kills = score_info.kills;
                                entry.deaths = score_info.deaths;
                            });
                        }
                    }
                }

                Ok("ScoreShort") => {
                    if let Ok((_, score_short)) = parse_score_short(user_msg.data) {
                        let player_idx = score_short.player_idx - 1;

                        if let Some(player_id) = player_id_cache.get(&player_idx) {
                            scoreboard
                                .entry(*player_id)
                                .and_modify(|entry| {
                                    entry.points = score_short.points;
                                    entry.kills = score_short.kills as u8;
                                    entry.deaths = score_short.deaths as u8;
                                })
                                .or_insert(ScoreboardEntry {
                                    name: format!("Player {}", player_id),
                                    points: score_short.points,
                                    kills: score_short.kills as u8,
                                    deaths: score_short.deaths as u8,
                                    team: Team::UNKNOWN,
                                });
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    });

    let mut scoreboard_entries = scoreboard
        .into_iter()
        .collect::<Vec<(u32, ScoreboardEntry)>>();

    scoreboard_entries.sort_by(
        |(_, first), (_, second)| match (&first.team, &second.team) {
            (Team::ALLIES, Team::AXIS) => Ordering::Less,
            (Team::AXIS, Team::ALLIES) => Ordering::Greater,
            (Team::ALLIES | Team::AXIS, Team::SPECTATORS) => Ordering::Less,
            (Team::SPECTATORS, Team::ALLIES | Team::AXIS) => Ordering::Greater,
            _ => first
                .points
                .cmp(&second.points)
                .reverse()
                .then(first.kills.cmp(&second.kills).reverse()),
        },
    );

    let header_row = format!(
        "{:<6} | {:30} | {:10} | {:<6} | {:<5} | {:<5}",
        "Id", "Name", "Team", "Points", "Kills", "Deaths"
    );

    println!("{}", header_row);
    println!("{}", "-".repeat(header_row.len()));

    scoreboard_entries.iter().for_each(|(player_id, entry)| {
        println!(
            "{:<6} | {:30} | {:10} | {:<6} | {:<5} | {:<5}",
            player_id,
            entry.name,
            match entry.team {
                Team::ALLIES => "Allies",
                Team::AXIS => "Axis",
                Team::SPECTATORS => "Spectators",
                Team::UNKNOWN => "Unknown",
            },
            entry.points,
            entry.kills,
            entry.deaths
        )
    });
}
