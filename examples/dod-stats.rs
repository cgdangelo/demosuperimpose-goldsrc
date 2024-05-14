extern crate demosuperimpose_goldsrc;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::identity;
use std::env::args;
use std::fs::File;
use std::io::Read;
use std::str::from_utf8;

use demosuperimpose_goldsrc::netmsg_doer::parse_netmsg;
use demosuperimpose_goldsrc::netmsg_doer::utils::get_initial_delta;
use demosuperimpose_goldsrc::open_demo;
use demosuperimpose_goldsrc::types::EngineMessage::SvcUpdateUserInfo;
use demosuperimpose_goldsrc::types::Message::EngineMessage;
use demosuperimpose_goldsrc::types::Message::UserMessage;
use demosuperimpose_goldsrc::types::SvcNewUserMsg;

struct ScoreboardEntry(String, String, (u8, u8, u8));

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
                Ok("DeathMsg") => {
                    if let [killer_client_index, victim_client_index, ..] = user_message.data {
                        if *killer_client_index == 0 || *victim_client_index == 0 {
                            return;
                        }

                        let killer_idx = killer_client_index - 1;
                        let victim_idx = victim_client_index - 1;

                        scoreboard.entry(killer_idx).or_insert((0, 0, 0));
                        scoreboard.entry(victim_idx).or_insert((0, 0, 0));

                        if killer_idx != victim_idx {
                            scoreboard
                                .entry(killer_idx)
                                .and_modify(|(kills, _, teamkills)| {
                                    let killer_team = player_teams.get(&killer_idx);
                                    let player_team = player_teams.get(&victim_idx);

                                    let is_teamkill = match (killer_team, player_team) {
                                        (Some(a), Some(b)) => a == b,
                                        _ => false,
                                    };

                                    if !is_teamkill {
                                        *kills += 1;
                                    } else {
                                        *teamkills += 1;
                                    }
                                });
                        }

                        scoreboard.entry(victim_idx).and_modify(|(_, deaths, _)| {
                            *deaths += 1;
                        });
                    }
                }
                _ => {}
            }
        }
        EngineMessage(engine_message) => match engine_message {
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
