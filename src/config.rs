use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};
use strum::VariantArray;

use crate::{
    global::GAME_AGENTS,
    valo_types::{GameAgent, GameMap},
    DIALOG_THEME, DONT_SAVE_CONFIG,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub instalock_wait_ms: u64,
    pub map_agent_config: MapAgentConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            instalock_wait_ms: 500,
            map_agent_config: MapAgentConfig::None,
        }
    }
}

#[derive(Debug, Copy, Clone, VariantArray)]
enum PromptRandomInstalock {
    Never,
    Choose,
    Always,
}

impl Display for PromptRandomInstalock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PromptRandomInstalock::Never => "Never",
                PromptRandomInstalock::Choose => "Let me choose",
                PromptRandomInstalock::Always => "Always",
            }
        )
    }
}

impl Config {
    pub fn read() -> anyhow::Result<Self> {
        if std::fs::try_exists(&crate::CONFIG_FILES.config)
            .is_ok_and(|exists| !exists)
        {
            eprintln!("Config file does not exist, creating default config");
            log::warn!("Config file does not exist, creating default config");
            let cfg = Self::default();
            cfg.write()?;
            return Ok(cfg);
        }
        Ok(serde_json::from_slice(&std::fs::read(
            &crate::CONFIG_FILES.config,
        )?)?)
    }

    pub fn write(&self) -> anyhow::Result<()> {
        if DONT_SAVE_CONFIG.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        Ok(std::fs::write(
            &crate::CONFIG_FILES.config,
            serde_json::to_vec_pretty(&self)?,
        )?)
    }

    pub fn prompt_instalock_wait_ms(
        prev: Option<Config>,
    ) -> Self {
        let mut cfg = prev.unwrap_or_default();
        cfg.instalock_wait_ms = dialoguer::Input::<u64>::new()
            .with_prompt(format!("Instalock wait time in ms"))
            .default(cfg.instalock_wait_ms)
            .interact().unwrap();
        cfg
    }

    fn prompt_agent_config_for_each_map(
        agents: &Vec<GameAgent>,
        maps: &Vec<GameMap>,
        rndm: PromptRandomInstalock,
        default: bool,
    ) -> HashMap<String, AgentConfig> {
        let mut map_agent_cfg = HashMap::new();
        for map in maps {
            if let Some(agent_cfg) = Self::prompt_agent_config(
                agents,
                Some(&map.name.0),
                rndm,
                default,
            ) {
                map_agent_cfg.insert(map.name.0.clone(), agent_cfg);
            }
        }
        map_agent_cfg
    }

    fn prompt_agent_config(
        agents: &Vec<GameAgent>,
        map: Option<&str>,
        rndm: PromptRandomInstalock,
        default: bool,
    ) -> Option<AgentConfig> {
        let rndm = match rndm {
            PromptRandomInstalock::Never => false,
            PromptRandomInstalock::Choose => {
                Self::prompt_randomized_instalock()
            }
            PromptRandomInstalock::Always => true,
        };
        Self::select_agents(agents, map, default).map(|res| match res {
            Ok(agents) if agents.is_empty() => AgentConfig::None,
            Ok(agents) => {
                if rndm {
                    let mut agents: Vec<_> =
                        agents.into_iter().map(|a| a.name.0).collect();
                    agents.sort();
                    AgentConfig::RandomOf(agents)
                } else {
                    AgentConfig::Some(
                        agents.into_iter().map(|a| a.name.0).collect(),
                    )
                }
            }
            Err(_) => {
                if rndm {
                    AgentConfig::Random
                } else {
                    // picking 'All agents' when not random is stupid
                    AgentConfig::None
                }
            }
        })
    }
    /*

    AgentConfig::Some(
                Self::select_agents(agents, map, default)?
                    .into_iter()
                    .map(|a| a.uuid)
                    .collect(),
            )

            if Self::prompt_randomized_instalock()? {
                    match Self::select_agents_or_all(agents, map, default)? {
                        Ok(agents) => {
                            AgentConfig::RandomOf(agents.into_iter().map(|a| a.uuid).collect())
                        },
                        Err(_) => {
                            AgentConfig::Random
                        },
                    }
                } else {
                    AgentConfig::Some(
                        Self::select_agents(agents, map, default)?
                            .into_iter()
                            .map(|a| a.uuid)
                            .collect(),
                    )
                }

     */

    // fn select_agents(
    //     agents: &Vec<GameAgent>,
    //     map: Option<&str>,
    //     default: bool,
    // ) -> Option<Vec<GameAgent>> {
    //     let mut prompt = match default {
    //         true => format!("Select default Agent(s)"),
    //         false => format!("Select Agent(s)"),
    //     };
    //     if let Some(map) = map {
    //         prompt.push_str(&format!(" for {}", map))
    //     }

    //     dialoguer::MultiSelect::with_theme(&*DIALOG_THEME)
    //         .with_prompt(prompt)
    //         .items(agents)
    //         .interact_opt()
    //         .unwrap()
    //         .map(|v| {
    //             v.into_iter().map(|i| agents[i].clone()).collect::<Vec<_>>()
    //         })
    // }

    fn select_agents(
        agents: &Vec<GameAgent>,
        map: Option<&str>,
        default: bool,
    ) -> Option<Result<Vec<GameAgent>, ()>> {
        let mut prompt = match default {
            true => format!("Select default Agent(s)"),
            false => format!("Select Agent(s)"),
        };
        if let Some(map) = map {
            assert!(!default);
            prompt.push_str(&format!(" for {}", map))
        }

        let items = Some("No Agents".to_string())
            .into_iter()
            .chain(Some("All Agents".to_string()))
            .chain(agents.iter().map(|a| a.to_string()))
            .collect::<Vec<_>>();
        dialoguer::MultiSelect::with_theme(&*DIALOG_THEME)
            .with_prompt(prompt)
            .items(&items)
            .interact_opt()
            .unwrap()
            .map(|v| {
                if v.contains(&0) {
                    // select 'No Agents'
                    Ok(vec![])
                } else if v.contains(&1) {
                    // select 'All Agents'
                    Err(())
                } else {
                    // select actual Agents or empty
                    Ok(v.into_iter().map(|i| agents[i - 2].clone()).collect())
                }
            })
    }

    fn prompt_randomized_instalock_generic() -> Option<PromptRandomInstalock> {
        dialoguer::Select::with_theme(&*DIALOG_THEME).with_prompt("Do you want to Instalock one of your selected Agents at random?").items(PromptRandomInstalock::VARIANTS).interact_opt().unwrap().map(|i| PromptRandomInstalock::VARIANTS[i])
    }

    fn prompt_randomized_instalock() -> bool {
        dialoguer::Confirm::with_theme(&*DIALOG_THEME)
            .with_prompt(
                "Do you want to Instalock from your selection at random?",
            )
            .default(false)
            .interact()
            .unwrap()
    }

    fn prompt_map_agent_cfg_kind(
    ) -> Option<(MapAgentConfigKind, PromptRandomInstalock)> {
        let kind = dialoguer::Select::with_theme(&*DIALOG_THEME)
            .with_prompt(
                "How do you want to configure the Instalock Agent selection?",
            )
            .items(MapAgentConfigKind::VARIANTS)
            .interact_opt()
            .map(|i| i.map(|i| MapAgentConfigKind::VARIANTS[i]))
            .unwrap()?;
        Some((
            kind,
            match kind {
                MapAgentConfigKind::None => PromptRandomInstalock::Never,
                _ => Self::prompt_randomized_instalock_generic()?,
            },
        ))
    }

    fn select_maps(maps: &Vec<GameMap>) -> Option<Vec<MapName>> {
        dialoguer::MultiSelect::with_theme(&*DIALOG_THEME)
            .with_prompt("Select Maps")
            .items(maps)
            .interact_opt()
            .unwrap()
            .map(|v| {
                let mut maps: Vec<_> =
                    v.into_iter().map(|i| maps[i].name.0.clone()).collect();
                maps.sort();
                maps
            })
    }

    pub fn prompt_map_agent_cfg(
        prev: Option<Config>,
        maps: &Vec<GameMap>,
        agents: &Vec<GameAgent>,
    ) -> Option<Self> {
        let mut cfg = prev.unwrap_or_default();
        cfg.map_agent_config = match Self::prompt_map_agent_cfg_kind() {
            None => cfg.map_agent_config,
            Some((MapAgentConfigKind::None, _)) => MapAgentConfig::None,
            Some((MapAgentConfigKind::Default, rndm)) => {
                // prompt default agent only
                MapAgentConfig::Default(Self::prompt_agent_config(
                    agents, None, rndm, true,
                )?)
            }
            Some((MapAgentConfigKind::PerSelectedMap, rndm)) => {
                // prompt agent for each map
                MapAgentConfig::PerSelectedMap {
                    map_agents: Self::prompt_agent_config_for_each_map(
                        agents, maps, rndm, false,
                    ),
                }
            }
            Some((MapAgentConfigKind::DefaultOnSelectedMaps, rndm)) => {
                MapAgentConfig::DefaultOnSelectedMaps {
                    default: Self::prompt_agent_config(
                        agents, None, rndm, true,
                    )?,
                    maps: Self::select_maps(maps)?,
                }
                // prompt once for default agent, then multiselect maps
            }
            Some((MapAgentConfigKind::PerSelectedMapOrDefault, rndm)) => {
                // prompt default agent, then agent for each map
                MapAgentConfig::PerSelectedMapOrDefault {
                    default: Self::prompt_agent_config(
                        agents, None, rndm, true,
                    )?,
                    map_agents: Self::prompt_agent_config_for_each_map(
                        agents, maps, rndm, false,
                    ),
                }
            }
        };
        Some(cfg)
    }

    pub fn get_agents(&self, map_name: &str) -> Vec<GameAgent> {
        match &self.map_agent_config {
            MapAgentConfig::None => vec![],
            MapAgentConfig::Default(agents) => agents.get_agents(),
            MapAgentConfig::PerSelectedMap { map_agents } => map_agents
                .get(map_name)
                .map_or(vec![], |cfg| cfg.get_agents()),
            MapAgentConfig::DefaultOnSelectedMaps { default, maps } => {
                if maps
                    .iter()
                    .map(|a| a.as_str())
                    .find(|a| a == &map_name)
                    .is_some()
                {
                    default.get_agents()
                } else {
                    vec![]
                }
            }
            MapAgentConfig::PerSelectedMapOrDefault {
                default,
                map_agents,
            } => map_agents
                .get(map_name)
                .map_or(default.get_agents(), |cfg| cfg.get_agents()),
        }
    }
}

pub type MapName = String;
pub type AgentName = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentConfig {
    None,
    Some(Vec<AgentName>),
    Random,
    RandomOf(Vec<AgentName>),
}

impl AgentConfig {
    pub fn get_agents(&self) -> Vec<GameAgent> {
        use rand::prelude::SliceRandom;
        match self {
            AgentConfig::None => vec![],
            AgentConfig::Some(agents) => GAME_AGENTS
                .get()
                .unwrap()
                .iter()
                .filter(|a| agents.contains(&a.name.0))
                .map(|a| a.clone())
                .collect(),
            AgentConfig::Random => {
                let mut agents = GAME_AGENTS.get().unwrap().clone();
                agents.shuffle(&mut rand::thread_rng());
                agents
            }
            AgentConfig::RandomOf(agents) => {
                let mut agents = GAME_AGENTS
                    .get()
                    .unwrap()
                    .iter()
                    .filter(|a| agents.contains(&a.name.0))
                    .map(|a| a.clone())
                    .collect::<Vec<_>>();
                agents.shuffle(&mut rand::thread_rng());
                agents
            }
        }
    }
}

impl Display for AgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentConfig::None => write!(f, "Disabled"),
            AgentConfig::Some(agents) => {
                for (i, agent) in agents.iter().enumerate() {
                    write!(f, "{}. {}, ", i + 1, agent)?;
                }
                Ok(())
            }
            AgentConfig::Random => write!(f, "Random Agent"),
            AgentConfig::RandomOf(agents) => {
                write!(f, "Random from ")?;
                for agent in agents {
                    write!(f, "{}, ", agent)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum MapAgentConfig {
    #[default]
    None,
    Default(AgentConfig),
    PerSelectedMap {
        map_agents: HashMap<MapName, AgentConfig>,
    },
    DefaultOnSelectedMaps {
        default: AgentConfig,
        maps: Vec<MapName>,
    },
    PerSelectedMapOrDefault {
        default: AgentConfig,
        map_agents: HashMap<MapName, AgentConfig>,
    },
}

impl Display for MapAgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapAgentConfig::None => write!(f, "Disabled"),
            MapAgentConfig::Default(cfg) => {
                write!(f, "Default = {}", cfg)
            }
            // EXAMPLE:
            // Custom Agents for select Maps =
            //   Bind: 1. Sova, 2. Chamber, 3. Reyna,
            //   Haven: Random from: Sova, Chamber, Reyna,
            //   Split: Random Agent,
            //   Icebox: All Agents,
            MapAgentConfig::PerSelectedMap { map_agents } => {
                writeln!(f, "Custom agents for select maps = ")?;
                for (map, cfg) in map_agents {
                    writeln!(f, "  {}: {}", map, cfg)?;
                }
                Ok(())
            }
            MapAgentConfig::DefaultOnSelectedMaps { default, maps } => {
                writeln!(f, "Default agents for select maps = {default}")?;
                write!(f, "  ")?;
                for map in maps {
                    write!(f, "{}, ", map)?;
                }
                Ok(())
            }
            MapAgentConfig::PerSelectedMapOrDefault {
                default,
                map_agents,
            } => {
                writeln!(
                    f,
                    "Custom agents for select maps or default = {default}"
                )?;
                for (map, cfg) in map_agents {
                    writeln!(f, "  {}: {}", map, cfg)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Default, VariantArray)]
pub enum MapAgentConfigKind {
    #[default]
    None,
    Default,
    PerSelectedMap,
    DefaultOnSelectedMaps,
    PerSelectedMapOrDefault,
}

impl Display for MapAgentConfigKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapAgentConfigKind::None => write!(f, "Disabled"),
            MapAgentConfigKind::Default => {
                write!(f, "Default Agents on all Maps")
            }
            MapAgentConfigKind::PerSelectedMap => {
                write!(f, "Custom Agents on certain Maps only")
            }
            MapAgentConfigKind::DefaultOnSelectedMaps => {
                write!(f, "Default Agents on certain Maps only")
            }
            MapAgentConfigKind::PerSelectedMapOrDefault => write!(
                f,
                "Custom Agents on certain Maps, Default Agents on the rest"
            ),
        }
    }
}
