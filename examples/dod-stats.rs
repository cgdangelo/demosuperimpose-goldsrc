extern crate demosuperimpose_goldsrc;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::identity;
use std::env::args;
use std::fs::File;
use std::io::Read;
use std::str::from_utf8;

use nom::number::complete::{be_u16, le_u8};
use nom::sequence::tuple;
use nom::{IResult, Parser};

use demosuperimpose_goldsrc::netmsg_doer::parse_netmsg;
use demosuperimpose_goldsrc::netmsg_doer::utils::get_initial_delta;
use demosuperimpose_goldsrc::open_demo;
use demosuperimpose_goldsrc::types::EngineMessage::SvcUpdateUserInfo;
use demosuperimpose_goldsrc::types::Message::UserMessage;
use demosuperimpose_goldsrc::types::{Message, SvcNewUserMsg};

struct ScoreboardEntry(String, String, (u8, u8, u8));

#[derive(Debug)]
struct DeathMsg {
    killer_idx: u8,
    victim_idx: u8,
    weapon_idx: u8,
}

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

#[derive(Debug)]
enum Team {
    ALLIES,
    AXIS,
    SPECTATOR,
    DISCONNECTED,
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

    let mut player_names: HashMap<u8, &str> = HashMap::new();
    let mut player_teams: HashMap<u8, &str> = HashMap::new();
    let mut scoreboard: HashMap<u8, (u8, u8, u8)> = HashMap::new();

    network_messages.for_each(|message| match message {
        UserMessage(user_message) => {
            match from_utf8(user_message.name).map(|s| s.trim_matches('\0')) {
                Ok(msg_name) => {
                    match msg_name {
                        "Frags" => {
                            println!("{} {:?}", msg_name, user_message.data);

                            if let Ok((_, frags)) = parse_frags(user_message.data) {
                                println!("{:?}", frags);

                                scoreboard
                                    .entry(frags.player_idx - 1)
                                    .and_modify(|(kills, _, _)| {
                                        *kills = frags.value;
                                    })
                                    .or_insert((frags.value, 0, 0));
                            }
                        }

                        "ScoreInfo" => {
                            println!("{} {:?}", msg_name, user_message.data);

                            if let Ok((_, score_info)) = parse_score_info(user_message.data) {
                                println!("{:?}", score_info);

                                scoreboard.insert(
                                    score_info.player_idx - 1,
                                    (score_info.kills, score_info.deaths, 0),
                                );
                            }
                        }

                        "ScoreShort" => {
                            println!("{} {:?}", msg_name, user_message.data);

                            if let Ok((_, score_short)) = parse_score_short(user_message.data) {
                                println!("{:?}", score_short);

                                scoreboard.insert(
                                    score_short.player_idx - 1,
                                    (score_short.kills as u8, score_short.deaths as u8, 0),
                                );
                            }
                        }

                        "DeathMsg" | "RoundState" => {
                            println!("{:10} {:?}", msg_name, user_message.data)
                        }
                        _ => {} // "AmmoX" | "BloodPuff" | "ClCorpse" | "CurWeapon" | "Health"
                                // | "HideWeapon" | "InitHUD" | "MOTD" | "PClass" | "ReloadDone"
                                // | "ResetHUD" | "ResetSens" | "Scope" | "ScreenFade" | "ScreenShake"
                                // | "ServerName" | "SetFOV" | "Spectator" | "TextMsg" | "VGUIMenu"
                                // | "VoiceMask" | "WaveStatus" | "WaveTime" | "WeaponList" => {}
                                // msg_name => {
                                //     println!("{:10} {:?}", msg_name, user_message.data)
                                // }
                    }
                    // ScoreInfo
                    // - 1 player_idx
                    // - 2 unk
                    // - 3 unk
                    // - 4 unk
                    // - 5 unk
                    // - 6 unk
                    // - 7 team_id
                }

                // Ok("DeathMsg") => {
                //     println!("{} {:?}", "DeathMsg", user_message.data);
                //
                //     if let [killer_client_index, victim_client_index, ..] = user_message.data {
                //         if *killer_client_index == 0 || *victim_client_index == 0 {
                //             return;
                //         }
                //
                //         let killer_idx = killer_client_index - 1;
                //         let victim_idx = victim_client_index - 1;
                //
                //         scoreboard.entry(killer_idx).or_insert((0, 0, 0));
                //         scoreboard.entry(victim_idx).or_insert((0, 0, 0));
                //
                //         if killer_idx != victim_idx {
                //             scoreboard
                //                 .entry(killer_idx)
                //                 .and_modify(|(kills, _, teamkills)| {
                //                     let killer_team = player_teams.get(&killer_idx);
                //                     let player_team = player_teams.get(&victim_idx);
                //
                //                     let is_teamkill = match (killer_team, player_team) {
                //                         (Some(a), Some(b)) => a == b,
                //                         _ => false,
                //                     };
                //
                //                     if !is_teamkill {
                //                         *kills += 1;
                //                     } else {
                //                         *teamkills += 1;
                //                     }
                //                 });
                //         }
                //
                //         scoreboard.entry(victim_idx).and_modify(|(_, deaths, _)| {
                //             *deaths += 1;
                //         });
                //     }
                // }
                _ => {}
            }
        }
        Message::EngineMessage(engine_message) => match engine_message {
            SvcUpdateUserInfo(msg) => {
                let info = from_utf8(msg.user_info)
                    .map(|s| s.trim_matches(['\0', '\\']))
                    .map(|s| s.split("\\").collect::<Vec<&str>>())
                    .unwrap();

                info.iter()
                    .enumerate()
                    .step_by(2)
                    .for_each(|(i, v)| match *v {
                        "name" => {
                            if let Some(name) = info.get(i + 1) {
                                player_names.insert(msg.index, name);
                            }
                        }
                        "team" => {
                            if let Some(team) = info.get(i + 1) {
                                player_teams.insert(msg.index, team);
                            }
                        }
                        _ => {}
                    })
            }
            _ => {}
        },
    });

    let mut scoreboard_entries: Vec<ScoreboardEntry> = scoreboard
        .iter()
        .filter_map(|(player_idx, scores)| {
            if *player_idx == 0 {
                return None;
            }

            let mapped_name = match player_names.get(player_idx) {
                Some(player_name) => player_name.to_string(),
                None => format!("Player {}", player_idx),
            };

            let mapped_team = match player_teams.get(player_idx) {
                Some(team) => format!(
                    "{}",
                    match *team {
                        "allies" => "Allies",
                        "axis" => "Axis",
                        _ => "Spectator",
                    }
                ),
                None => "Unknown".to_string(),
            };

            Some(ScoreboardEntry(mapped_name, mapped_team, scores.to_owned()))
        })
        .collect();

    scoreboard_entries.sort_by(
        |ScoreboardEntry(_, a_team, (a_kills, _, _)),
         ScoreboardEntry(_, b_team, (b_kills, _, _))| {
            match (a_team.as_str(), b_team.as_str()) {
                ("Axis", "Allies") => Ordering::Greater,
                ("Allies", "Axis") => Ordering::Less,
                _ => a_kills.cmp(b_kills).reverse(),
            }
        },
    );

    scoreboard_entries.iter().for_each(
        |ScoreboardEntry(player_name, team, (kills, deaths, teamkills))| {
            println!(
                "{:24} | {:6} | K:{:<2} | D:{:<2} | TK:{:<2}",
                player_name, team, kills, deaths, teamkills
            );
        },
    );
}
