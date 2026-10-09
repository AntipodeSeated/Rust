use std::cmp::Ordering;

use iced::{application, Element, Length, Task};
use iced::widget::{button, column, pick_list, row, scrollable, text, text_input};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SearchMode {
    #[default]
    System,
    Playfield,
    Entity,
}

impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchMode::System => write!(f, "System"),
            SearchMode::Playfield => write!(f, "Playfield"),
            SearchMode::Entity => write!(f, "Entity"),
        }
    }
}

impl SearchMode {
    fn label(&self) -> &'static str {
        match self {
            SearchMode::System => "System",
            SearchMode::Playfield => "Playfield",
            SearchMode::Entity => "Entity",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SortColumn {
    #[default]
    Distance,
    System,
    Playfield,
    Entity,
}

#[derive(Clone, Debug)]
struct SearchResult {
    distance: f64,
    system: String,
    playfield: String,
    entity: String,
}

#[derive(Debug, Clone)]
enum Message {
    LoadSettings,
    PickDbPath,
    DbPathChanged(String),
    SourceSystemChanged(String),
    SearchTextChanged(String),
    SearchModeChanged(SearchMode),
    Search,
    RefreshSourceSystem,
    SortBy(SortColumn),
    SetError(String),
}

#[derive(Debug, Default)]
struct App {
    db_path: String,
    source_system: String,
    search_text: String,
    search_mode: SearchMode,
    source_x: f64,
    source_y: f64,
    source_z: f64,
    player_system: String,
    results: Vec<SearchResult>,
    status: String,
    sort_column: SortColumn,
    sort_ascending: bool,
}

fn sort_results(results: &mut [SearchResult], column: SortColumn, ascending: bool) {
    results.sort_by(|left, right| {
        let comparison = match column {
            SortColumn::Distance => left.distance.partial_cmp(&right.distance).unwrap_or(Ordering::Equal),
            SortColumn::System => left
                .system
                .to_lowercase()
                .cmp(&right.system.to_lowercase()),
            SortColumn::Playfield => left
                .playfield
                .to_lowercase()
                .cmp(&right.playfield.to_lowercase()),
            SortColumn::Entity => left
                .entity
                .to_lowercase()
                .cmp(&right.entity.to_lowercase()),
        };

        if ascending {
            comparison
        } else {
            comparison.reverse()
        }
    });
}

fn load_saved_db_path() -> Option<String> {
    let contents = std::fs::read_to_string("settings.config").ok()?;

    contents
        .lines()
        .find_map(|line| line.strip_prefix("db_path=").map(str::to_owned))
}

fn save_db_path(path: &str) {
    let _ = std::fs::write("settings.config", format!("db_path={}\n", path));
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::LoadSettings => {
            let saved_path = load_saved_db_path().unwrap_or_default();
            if !saved_path.is_empty() {
                app.db_path = saved_path.clone();
                return Task::done(Message::RefreshSourceSystem);
            }
            Task::none()
        }
        Message::PickDbPath => {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Select database file")
                .add_filter("Database", &["db"])
                .set_directory(std::env::current_dir().unwrap_or_default())
                .pick_file()
            {
                return Task::done(Message::DbPathChanged(
                    path.to_string_lossy().into_owned(),
                ));
            }

            Task::none()
        }
        Message::DbPathChanged(path) => {
            app.db_path = path.clone();
            save_db_path(&path);
            Task::done(Message::RefreshSourceSystem)
        }
        Message::SourceSystemChanged(value) => {
            app.source_system = value;
            Task::none()
        }
        Message::SearchTextChanged(value) => {
            app.search_text = value;
            Task::none()
        }
        Message::SearchModeChanged(mode) => {
            app.search_mode = mode;
            Task::none()
        }
        Message::RefreshSourceSystem => {
            if app.db_path.trim().is_empty() {
                return Task::none();
            }

            match rusqlite::Connection::open(&app.db_path) {
                Ok(conn) => {
                    let mut stmt = match conn.prepare(
                        "select s.name [System], p.name [Location], s.sectorx, s.sectory, s.sectorz from SolarSystems s join Playfields p on s.ssid = p.ssid join PlayerData d on d.pfid = p.pfid;",
                    ) {
                        Ok(stmt) => stmt,
                        Err(error) => return Task::done(Message::SetError(error.to_string())),
                    };

                    let first_row = match stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, f64>(2)?,
                            row.get::<_, f64>(3)?,
                            row.get::<_, f64>(4)?,
                        ))
                    }) {
                        Ok(rows) => rows.into_iter().next(),
                        Err(error) => return Task::done(Message::SetError(error.to_string())),
                    };

                    match first_row {
                        Some(result) => {
                            match result {
                                Ok((system, sector_x, sector_y, sector_z)) => {
                                    app.source_system = system.clone();
                                    app.player_system = system;
                                    app.source_x = sector_x;
                                    app.source_y = sector_y;
                                    app.source_z = sector_z;
                                    Task::none()
                                }
                                Err(error) => Task::done(Message::SetError(error.to_string())),
                            }
                        }
                        None => Task::done(Message::SetError(
                            "No player position found for the selected database.".to_string(),
                        )),
                    }
                }
                Err(error) => Task::done(Message::SetError(error.to_string())),
            }
        }
        Message::Search => {
            if app.db_path.trim().is_empty() {
                return Task::done(Message::SetError("No database path selected.".to_string()));
            }

            let search_term = format!("%{}%", app.search_text);
            let query = match app.search_mode {
                SearchMode::System => {
                    "select s.name [System], '' [Playfield], '' [Entity], s.sectorx, s.sectory, s.sectorz from SolarSystems s where s.name like ?;"
                }
                SearchMode::Playfield => {
                    "select s.name [System], p.name [Playfield], '' [Entity], s.sectorx, s.sectory, s.sectorz from SolarSystems s join Playfields p on s.ssid = p.ssid where p.name like ?;"
                }
                SearchMode::Entity => {
                    "select s.name [System], p.name [Playfield], e.name [Entity], s.sectorx, s.sectory, s.sectorz from SolarSystems s join Playfields p on s.ssid = p.ssid join Entities e on e.pfid = p.pfid where e.name like ?;"
                }
            };

            match rusqlite::Connection::open(&app.db_path) {
                Ok(conn) => {
                    let mut stmt = match conn.prepare(query) {
                        Ok(stmt) => stmt,
                        Err(error) => return Task::done(Message::SetError(error.to_string())),
                    };

                    let rows = match stmt.query_map([search_term], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, f64>(3)?,
                            row.get::<_, f64>(4)?,
                            row.get::<_, f64>(5)?,
                        ))
                    }) {
                        Ok(rows) => rows,
                        Err(error) => return Task::done(Message::SetError(error.to_string())),
                    };

                    let mut results = Vec::new();
                    for row in rows {
                        match row {
                            Ok((system, playfield, entity, sector_x, sector_y, sector_z)) => {
                                let distance = ((app.source_x - sector_x).powi(2)
                                    + (app.source_y - sector_y).powi(2)
                                    + (app.source_z - sector_z).powi(2))
                                    .sqrt()
                                    / 100_000.0;

                                results.push(SearchResult {
                                    distance: (distance * 100.0).round() / 100.0,
                                    system,
                                    playfield,
                                    entity,
                                });
                            }
                            Err(error) => return Task::done(Message::SetError(error.to_string())),
                        }
                    }

                    sort_results(&mut results, app.sort_column, app.sort_ascending);
                    app.results = results;
                    Task::none()
                }
                Err(error) => Task::done(Message::SetError(error.to_string())),
            }
        }
        Message::SortBy(column) => {
            if app.sort_column == column {
                app.sort_ascending = !app.sort_ascending;
            } else {
                app.sort_column = column;
                app.sort_ascending = true;
            }

            sort_results(&mut app.results, app.sort_column, app.sort_ascending);
            Task::none()
        }
        Message::SetError(error) => {
            app.status = error;
            Task::none()
        }
    }
}

fn view(app: &App) -> Element<'_, Message> {
    let top_row = row![
        text("db Path:"),
        text_input("Global.db path", &app.db_path)
            .on_input(Message::DbPathChanged)
            .width(Length::Fill),
        button("Browse").on_press(Message::PickDbPath),
    ]
    .spacing(10)
    .padding(10);

    let second_row = row![
        text("Source System:"),
        text_input("Source System", &app.source_system)
            .on_input(Message::SourceSystemChanged)
            .width(Length::FillPortion(2)),
        text("Search:"),
        text_input("search term", &app.search_text)
            .on_input(Message::SearchTextChanged)
            .width(Length::FillPortion(2)),
        pick_list(
            [SearchMode::System, SearchMode::Playfield, SearchMode::Entity],
            Some(app.search_mode),
            Message::SearchModeChanged,
        ),
        button("Search").on_press(Message::Search),
    ]
    .spacing(10)
    .padding(10);

    let header = row![
        button("Distance")
            .width(Length::Fixed(100.0))
            .on_press(Message::SortBy(SortColumn::Distance)),
        button("System")
            .width(Length::Fixed(200.0))
            .on_press(Message::SortBy(SortColumn::System)),
        button("Playfield")
            .width(Length::Fixed(180.0))
            .on_press(Message::SortBy(SortColumn::Playfield)),
        button("Entity")
            .width(Length::Fixed(220.0))
            .on_press(Message::SortBy(SortColumn::Entity)),
    ]
    .spacing(12)
    .padding([6, 8]);

    let result_rows = app.results.iter().map(|row| {
        row![
            text(format!("{:.2}", row.distance)).width(Length::Fixed(100.0)),
            text(&row.system).width(Length::Fixed(200.0)),
            text(&row.playfield).width(Length::Fixed(180.0)),
            text(&row.entity).width(Length::Fixed(220.0)),
        ]
        .spacing(12)
        .padding([4, 8])
        .into()
    });

    let results = scrollable(column![header, column(result_rows)]).height(Length::Fill);

    column![top_row, second_row, results, text(&app.status)]
        .spacing(10)
        .padding(12)
        .into()
}

fn main() -> iced::Result {
    application("Andromeda Search", update, view).run_with(|| {
        (App::default(), Task::done(Message::LoadSettings))
    })
}
