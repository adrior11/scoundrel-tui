use std::{
    collections::VecDeque,
    fmt::{Display, Formatter},
    fs,
    io::Stdout,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use arboard::Clipboard;
use chrono::{Local, TimeZone};
use directories::ProjectDirs;
use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};
use ratatui::{
    Terminal,
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Direction, Flex, HorizontalAlignment, Layout, Rect},
    prelude::{CrosstermBackend, Widget},
    style::Style,
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, ListState, Padding, Paragraph, Row,
        Table, Wrap,
    },
};
use serde::{Deserialize, Serialize};
use tui_textarea::{Input, TextArea};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const DECK_SIZE: usize = 44;
const MAX_SCORES: usize = 10;
const MAX_HEALTH: u8 = 20;
const MESSAGE_CAP: usize = 10;

const MIN_TERM_WIDTH: u16 = 65;
const MIN_TERM_HEIGHT: u16 = 25;

const CARD_WIDTH: u16 = 7;
const CARD_HEIGHT: u16 = 5;
const CARD_SPACING: u16 = 1;

const GAME_MAX_WIDTH: u16 = 65;
const MENU_BODY_HEIGHT: u16 = 15;
const MENU_HOME_WIDTH: u16 = 13;
const MENU_SEED_WIDTH: u16 = 30;
const MENU_SEED_HEIGHT: u16 = 3;
const MENU_SCORES_WIDTH: u16 = 44;
const MENU_CLEAR_CONFIRM_WIDTH: u16 = 35;
const MENU_CLEAR_CONFIRM_HEIGHT: u16 = 7;

const LOGO: &str = r#"
       ...                                                               ..                                     .. 
   .x888888hx    :                                                     dF                                 x .d88"  
  d88888888888hxx                   u.      x.    .        u.    u.   '88bu.         .u    .               5888R   
 8" ... `"*8888%`        .    ...ue888b   .@88k  z88u    x@88k u@88c. '*88888bu    .d88B :@8c       .u     '888R   
!  "   ` .xnxx.     .udR88N   888R Y888r ~"8888 ^8888   ^"8888""8888"   ^"*8888N  ="8888f8888r   ud8888.    888R   
X X   .H8888888%:  <888'888k  888R I888>   8888  888R     8888  888R   beWE "888L   4888>'88"  :888'8888.   888R   
X 'hn8888888*"   > 9888 'Y"   888R I888>   8888  888R     8888  888R   888E  888E   4888> '    d888 '88%"   888R   
X: `*88888%`     ! 9888       888R I888>   8888  888R     8888  888R   888E  888E   4888>      8888.+"      888R   
'8h.. ``     ..x8> 9888      u8888cJ888    8888 ,888B .   8888  888R   888E  888F  .d888L .+   8888L        888R   
 `88888888888888f  ?8888u../  "*888*P"    "8888Y 8888"   "*88*" 8888" .888N..888   ^"8888*"    '8888c. .+  .888B . 
  '%8888888888*"    "8888P'     'Y"        `Y"   'YP       ""   'Y"    `"888*""       "Y"       "88888%    ^*888%  
     ^"****""`        "P'                                                 ""                      "YP'       "%    
"#;

#[derive(Clone, Copy)]
enum Suit {
    Hearts,
    Diamonds,
    Clubs,
    Spades,
}

impl Suit {
    const ALL: [Self; 4] = [Self::Hearts, Self::Diamonds, Self::Clubs, Self::Spades];
}

impl Display for Suit {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let suit_str = match self {
            Self::Hearts => "♥",
            Self::Diamonds => "♦",
            Self::Clubs => "♣",
            Self::Spades => "♠",
        };
        write!(f, "{}", suit_str)
    }
}

#[derive(Clone, Copy)]
enum Rank {
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}

impl Rank {
    fn short(self) -> &'static str {
        match self {
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
            Self::Ace => "A",
        }
    }

    fn value(self) -> u8 {
        match self {
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
            Self::Eight => 8,
            Self::Nine => 9,
            Self::Ten => 10,
            Self::Jack => 11,
            Self::Queen => 12,
            Self::King => 13,
            Self::Ace => 14,
        }
    }

    fn from_value(value: u8) -> Option<Self> {
        match value {
            2 => Some(Self::Two),
            3 => Some(Self::Three),
            4 => Some(Self::Four),
            5 => Some(Self::Five),
            6 => Some(Self::Six),
            7 => Some(Self::Seven),
            8 => Some(Self::Eight),
            9 => Some(Self::Nine),
            10 => Some(Self::Ten),
            11 => Some(Self::Jack),
            12 => Some(Self::Queen),
            13 => Some(Self::King),
            14 => Some(Self::Ace),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Styled<W> {
    inner: W,
    style: Style,
}

impl<W: Widget> Styled<W> {
    fn new(inner: W, style: Style) -> Self {
        Self { inner, style }
    }
}

impl<W: Widget> Widget for Styled<W> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, self.style);
        self.inner.render(area, buf);
    }
}

#[derive(Clone, Copy)]
struct Card {
    suit: Suit,
    rank: Rank,
}

impl Card {
    fn new(suit: Suit, rank: Rank) -> Self {
        Self { suit, rank }
    }

    fn dimmed(self) -> Styled<Self> {
        Styled::new(self, Styling::Dim.into())
    }
}

impl Widget for Card {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let top = Line::from(format!("{}{}", self.suit, " ".repeat(4)));
        let mid = Line::from(centered_str(self.rank.short(), 5));
        let bot = Line::from(format!("{}{}", " ".repeat(4), self.suit));

        Paragraph::new(vec![top, mid, bot])
            .block(Styling::card_block())
            .centered()
            .render(area, buf);
    }
}

#[derive(Clone, Copy)]
struct EmptyCard;

impl Widget for EmptyCard {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new("")
            .block(Styling::card_block_dimmed())
            .render(area, buf);
    }
}

struct Deck {
    cards: VecDeque<Card>,
}

impl Deck {
    fn new() -> Self {
        let mut cards: Vec<Card> = Vec::with_capacity(DECK_SIZE);

        for suit in Suit::ALL {
            let upper_bound = match suit {
                Suit::Hearts | Suit::Diamonds => 10,
                Suit::Clubs | Suit::Spades => 14,
            };

            for value in 2..=upper_bound {
                if let Some(rank) = Rank::from_value(value) {
                    cards.push(Card::new(suit, rank));
                } else {
                    panic!("Invalid rank value: {}", value);
                }
            }
        }

        Self {
            cards: cards.into(),
        }
    }

    fn draw(&mut self) -> Card {
        debug_assert!(!self.cards.is_empty(), "Cannot draw from an empty deck");
        self.cards.pop_front().expect("Deck is empty")
    }

    fn shuffle<R: Rng + ?Sized>(&mut self, rng: &mut R) {
        let mut v: Vec<_> = self.cards.drain(..).collect();
        v.shuffle(rng);
        self.cards = v.into();
    }

    fn len(&self) -> usize {
        self.cards.len()
    }

    fn place_cards_at_bottom(&mut self, cards: Vec<Card>) {
        self.cards.extend(cards);
        debug_assert!(self.cards.len() <= DECK_SIZE);
    }
}

struct Player {
    health: u8,
    weapon: Option<Card>,
    slain_cards: Vec<Card>,
    weapon_stance: bool,
}

impl Player {
    fn new() -> Self {
        Self {
            health: MAX_HEALTH,
            weapon: None,
            slain_cards: Vec::new(),
            weapon_stance: false,
        }
    }

    fn set_health(&mut self, health: u8) {
        self.health = health.min(MAX_HEALTH);
    }

    fn set_weapon(&mut self, card: Card) {
        self.weapon = Some(card);
    }

    fn drop_weapon(&mut self) -> Option<Card> {
        self.weapon.take()
    }

    fn add_slain_card(&mut self, card: Card) {
        self.slain_cards.push(card);
    }

    fn get_smallest_slain_card(&self) -> Option<&Card> {
        self.slain_cards.iter().min_by_key(|c| c.rank.value())
    }

    fn clear_slain_cards(&mut self) -> Vec<Card> {
        self.slain_cards.drain(..).collect()
    }

    fn toggle_weapon_usage(&mut self) {
        self.weapon_stance = !self.weapon_stance;
    }
}

#[derive(Clone, Copy)]
enum Slot {
    S1,
    S2,
    S3,
    S4,
}

impl Slot {
    const ALL: [Self; 4] = [Self::S1, Self::S2, Self::S3, Self::S4];

    fn idx(self) -> usize {
        match self {
            Self::S1 => 0,
            Self::S2 => 1,
            Self::S3 => 2,
            Self::S4 => 3,
        }
    }
}

struct Room {
    cards: [Option<Card>; 4],
}

impl Room {
    fn new() -> Self {
        Self {
            cards: [None, None, None, None],
        }
    }

    fn occupied_len(&self) -> usize {
        self.cards.iter().filter(|c| c.is_some()).count()
    }

    fn empty_slots(&self) -> Vec<Slot> {
        Slot::ALL
            .iter()
            .copied()
            .filter(|&slot: &Slot| self.cards[slot.idx()].is_none())
            .collect()
    }

    fn set(&mut self, slot: Slot, card: Card) {
        self.cards[slot.idx()] = Some(card);
    }

    fn take(&mut self, slot: Slot) -> Option<Card> {
        self.cards[slot.idx()].take()
    }

    fn peek(&self, slot: Slot) -> Option<&Card> {
        self.cards[slot.idx()].as_ref()
    }

    fn clear(&mut self) -> Vec<Card> {
        self.cards.iter_mut().filter_map(|c| c.take()).collect()
    }
}

impl Widget for &Room {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [label_area, slots_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(CARD_HEIGHT)])
            .areas(area);

        let room_label = Line::from(Span::styled("Room", Styling::Default));
        room_label.centered().render(label_area, buf);

        let slots = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(CARD_WIDTH); 4])
            .spacing(CARD_SPACING)
            .flex(Flex::Center)
            .split(slots_area);

        for slot in Slot::ALL {
            let area = slots[slot.idx()];
            match self.cards[slot.idx()] {
                Some(card) => card.render(area, buf),
                None => EmptyCard.render(area, buf),
            }
        }
    }
}

enum Phase {
    Dungeon,
    GameOver,
}

#[derive(Clone, Copy)]
enum Action {
    AvoidRoom,
    SelectSlot(Slot),
    ToggleWeaponUsage,
}

enum GameEvent {
    Log(String),
    Warn(String),
    GameOver { score: isize },
}

impl GameEvent {
    fn log(s: impl Into<String>) -> Self {
        Self::Log(s.into())
    }

    fn warn(s: impl Into<String>) -> Self {
        Self::Warn(s.into())
    }

    fn game_over(score: isize) -> Self {
        Self::GameOver { score }
    }
}

struct Game {
    deck: Deck,
    player: Player,
    room: Room,
    rooms_cleared: u32,
    discard_pile: Vec<Card>,

    avoid_available: bool,
    heal_available: bool,

    phase: Phase,
    seed: u64,
    seeded_run: bool,

    started_at: Instant,
    ended_at: Option<Instant>,
}

impl Game {
    fn new(seed: u64, seeded_run: bool) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut deck = Deck::new();
        deck.shuffle(&mut rng);
        Self {
            deck,
            player: Player::new(),
            room: Room::new(),
            rooms_cleared: 1,
            discard_pile: Vec::new(),
            avoid_available: true,
            heal_available: true,
            phase: Phase::Dungeon,
            seed,
            seeded_run,
            started_at: Instant::now(),
            ended_at: None,
        }
    }

    fn dispatch(&mut self, action: Action) -> Vec<GameEvent> {
        match self.phase {
            Phase::Dungeon => self.dispatch_dungeon(action),
            Phase::GameOver => vec![],
        }
    }

    fn dispatch_dungeon(&mut self, action: Action) -> Vec<GameEvent> {
        let mut events = Vec::new();

        match action {
            Action::AvoidRoom => {
                if !self.avoid_available {
                    events.push(GameEvent::warn("The dungeon won't spare you twice."));
                    return events;
                }

                if self.room.occupied_len() < 4 {
                    events.push(GameEvent::warn(
                        "You can't avoid now; you've already taken something from the room.",
                    ));
                    return events;
                }

                if self.deck.len() < 4 {
                    // only occurs in the final room
                    events.push(GameEvent::warn(
                        "No more sidestepping; this room demands a choice.",
                    ));
                    return events;
                }

                events.push(GameEvent::log(
                    "You avoid the room, leaving its contents to the dark.",
                ));
                self.avoid_room();
                self.refill_room();
                events
            }

            Action::ToggleWeaponUsage => {
                if self.player.weapon.is_none() {
                    events.push(GameEvent::warn(
                        "You reach for steel... but your hands find only air.",
                    ));
                    return events;
                }

                self.player.toggle_weapon_usage();
                events
            }

            Action::SelectSlot(slot) => {
                // Reject illegal weapon slays without mutating state:
                // If player is in weapon stance + has weapon + has already slain a card
                // + the selected card is a monster with rank >= smallest slain rank,
                // then do nothing and show a message.
                if self.player.weapon_stance && self.player.weapon.is_some() {
                    let Some(card) = self.room.peek(slot) else {
                        events.push(GameEvent::warn("Nothing there but cold stone."));
                        return events;
                    };

                    if matches!(card.suit, Suit::Spades | Suit::Clubs)
                        && let Some(last_slain) = self.player.get_smallest_slain_card()
                        && card.rank.value() >= last_slain.rank.value()
                    {
                        events.push(GameEvent::warn(format!(
                            "Your blade refuses the challenge: must slay weaker than {}{} (selected {}{}). Switch stance (Space) to fight bare-handed.",
                            last_slain.rank.short(),
                            last_slain.suit,
                            card.rank.short(),
                            card.suit,
                        )));
                        return events;
                    }
                }

                let Some(card) = self.room.take(slot) else {
                    events.push(GameEvent::warn("Nothing there but cold stone."));
                    return events;
                };

                events.extend(self.resolve_card(card));
                events
            }
        }
    }

    fn resolve_card(&mut self, card: Card) -> Vec<GameEvent> {
        let mut events = Vec::new();

        match card.suit {
            Suit::Spades | Suit::Clubs => {
                if self.player.weapon_stance {
                    let Some(weapon) = self.player.weapon else {
                        panic!("Player is using weapon but has no weapon equipped");
                    };

                    if let Some(last_slain) = self.player.get_smallest_slain_card() {
                        debug_assert!(
                            card.rank.value() < last_slain.rank.value(),
                            "Cannot slay a monster stronger than the smallest slain card using a weapon (new:{} vs last:{})",
                            card.rank.value(),
                            last_slain.rank.value()
                        );
                    }

                    let dmg = card.rank.value().saturating_sub(weapon.rank.value());
                    self.player
                        .set_health(self.player.health.saturating_sub(dmg));
                    self.player.add_slain_card(card);
                    if dmg == 0 {
                        events.push(GameEvent::log(format!(
                            "Your {}{} makes short work of {}{}. You take no damage.",
                            card.rank.short(),
                            card.suit,
                            weapon.rank.short(),
                            weapon.suit,
                        )));
                    } else {
                        events.push(GameEvent::log(format!(
                            "You bring {}{} down using {}{}. The counterstrike costs {} health.",
                            card.rank.short(),
                            card.suit,
                            weapon.rank.short(),
                            weapon.suit,
                            dmg
                        )));
                    }
                } else {
                    let dmg = card.rank.value();
                    self.player
                        .set_health(self.player.health.saturating_sub(dmg));
                    self.discard_pile.push(card);
                    events.push(GameEvent::log(format!(
                        "You smash into {}{} bare-handed. You suffer {} damage.",
                        card.rank.short(),
                        card.suit,
                        dmg
                    )));
                }
            }
            Suit::Hearts => {
                if self.heal_available {
                    let new_health = self.player.health + card.rank.value();
                    self.player.set_health(new_health);
                    self.heal_available = false;
                    events.push(GameEvent::log(
                        "You drink the potion. Warmth returns to your veins.",
                    ));
                } else {
                    events.push(GameEvent::warn(
                        "The room's magic is spent; you can't heal again here.",
                    ));
                }
                self.discard_pile.push(card);
            }
            Suit::Diamonds => {
                if self.player.weapon.is_none() {
                    self.player.weapon_stance = true;
                    events.push(GameEvent::warn(
                        "You arm yourself for the first time; weapon stance engaged.",
                    ));
                } else {
                    events.push(GameEvent::log(format!(
                        "You equip {}{}.",
                        card.rank.short(),
                        card.suit
                    )));
                }

                if let Some(weapon) = self.player.drop_weapon() {
                    self.discard_pile.push(weapon);
                }

                self.discard_pile.extend(self.player.clear_slain_cards());
                self.player.set_weapon(card);
            }
        }

        events.extend(self.check_game_over());
        self.assert_total_cards();
        events
    }

    fn check_game_over(&mut self) -> Vec<GameEvent> {
        // game lost condition
        if self.player.health == 0 {
            let score = self.get_score(false);
            self.phase = Phase::GameOver;
            self.finish_run();

            return vec![
                GameEvent::warn(format!(
                    "You collapse as the dungeon claims you at last. Score: {}",
                    score
                )),
                GameEvent::game_over(score),
            ];
        }

        // game won condition
        if self.deck.len() < 4 && self.room.occupied_len() == 1 {
            let score = self.get_score(true);
            self.phase = Phase::GameOver;
            self.finish_run();

            return vec![
                GameEvent::warn(format!(
                    "You emerge alive. The dungeon yields. Score: {}",
                    score
                )),
                GameEvent::game_over(score),
            ];
        }

        // otherwise advance dungeon if needed
        if self.room.occupied_len() <= 1 {
            self.rooms_cleared = self.rooms_cleared.saturating_add(1);
            self.refill_room();
            self.avoid_available = true;

            let msg = if self.deck.len() < 4 {
                "The air grows still. No paths remain beyond this chamber."
            } else {
                "A new room reveals itself ahead."
            };

            return vec![GameEvent::log(msg)];
        }

        vec![]
    }

    fn refill_room(&mut self) {
        for slot in self.room.empty_slots() {
            debug_assert!(
                !self.deck.cards.is_empty(),
                "Deck is empty when refilling room"
            );
            let card = self.deck.draw();
            self.room.set(slot, card);
        }
        self.heal_available = true;
        self.assert_total_cards();
    }

    /// Avoid the `Room` by scooping up all four cards and
    /// placing them at the bottom of the `Dungeon`.
    fn avoid_room(&mut self) {
        let cleared_cards = self.room.clear();
        self.deck.place_cards_at_bottom(cleared_cards);
        self.avoid_available = false;
        self.rooms_cleared = self.rooms_cleared.saturating_add(1);

        self.assert_total_cards();
    }

    /// Calculate the final score based on whether the player won or lost.
    ///
    /// If the player won, the score is the sum of the player's remaining health and the
    /// values of all Heart cards (`Potions`) in the room if the player's health is full.
    /// Otherwise, if the player lost, the score is the negative sum of the values
    /// of all Spade and Club cards (`Monsters`) remaining in the deck.
    fn get_score(&self, won: bool) -> isize {
        let mut score: isize = 0;

        if won {
            score += self.player.health as isize;
            if self.player.health == MAX_HEALTH {
                for c in self.room.cards.iter().flatten() {
                    if matches!(c.suit, Suit::Hearts) {
                        score += c.rank.value() as isize;
                    }
                }
            }
        } else {
            for c in &self.deck.cards {
                if matches!(c.suit, Suit::Spades | Suit::Clubs) {
                    score -= c.rank.value() as isize;
                }
            }
        }

        score
    }

    fn elapsed(&self) -> Duration {
        match self.ended_at {
            Some(end) => end.duration_since(self.started_at),
            None => Instant::now().duration_since(self.started_at),
        }
    }

    fn finish_run(&mut self) {
        if self.ended_at.is_none() {
            self.ended_at = Some(Instant::now());
        }
    }

    /// Invariant: The total number of cards in the game should always equal `DECK_SIZE`.
    fn assert_total_cards(&self) {
        let total_cards = self.deck.len()
            + self.room.cards.iter().filter(|c| c.is_some()).count()
            + self.discard_pile.len()
            + usize::from(self.player.weapon.is_some())
            + self.player.slain_cards.len();
        assert_eq!(
            total_cards, DECK_SIZE,
            "Total number of cards in game should always equal {}",
            DECK_SIZE
        );
    }
}

enum MessageKind {
    Log,
    Warn,
}

struct Message {
    kind: MessageKind,
    text: String,
}

struct MessageLog {
    cap: usize,
    items: VecDeque<Message>,

    dirty: bool,
    cached_width: usize,
    cached_lines: Vec<Line<'static>>,
}

impl MessageLog {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            items: VecDeque::with_capacity(cap),
            dirty: true,
            cached_width: 0,
            cached_lines: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.items.clear();
        self.dirty = true;
        self.cached_lines.clear();
    }

    fn push(&mut self, msg: MessageKind, text: impl Into<String>) {
        if self.items.len() == self.cap {
            self.items.pop_front();
        }
        self.items.push_back(Message {
            kind: msg,
            text: text.into(),
        });
        self.dirty = true;
    }

    fn rebuild_cache_if_needed(&mut self, width: usize) {
        if !self.dirty && self.cached_width == width {
            return;
        }
        self.cached_width = width;
        self.cached_lines.clear();

        for msg in &self.items {
            let style = match msg.kind {
                MessageKind::Log => Styling::Dim,
                MessageKind::Warn => Styling::Bold,
            };
            for chunk in wrap_to_width(&msg.text, width) {
                self.cached_lines
                    .push(Line::from(Span::styled(chunk, style)));
            }
        }

        self.dirty = false;
    }

    fn log(&mut self, text: impl Into<String>) {
        self.push(MessageKind::Log, text);
    }

    fn warn(&mut self, text: impl Into<String>) {
        self.push(MessageKind::Warn, text);
    }
}

impl Widget for &mut MessageLog {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner_w = area.width.saturating_sub(2) as usize;
        let inner_h = area.height.saturating_sub(2) as usize;

        self.rebuild_cache_if_needed(inner_w);

        let start = self.cached_lines.len().saturating_sub(inner_h);
        let view = &self.cached_lines[start..];

        Paragraph::new(view.to_vec())
            .block(Styling::block("Log"))
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

#[derive(Serialize, Deserialize)]
struct ScoreEntry {
    score: isize,
    rooms: u32,
    duration_ms: u64,
    finished_at_unix: i64,
    seeded: bool,
}

impl ScoreEntry {
    fn from_game(game: &Game, score: isize) -> Self {
        let duration_ms = u64::try_from(game.elapsed().as_millis()).expect("Duration too long");
        let finished_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .cast_signed();

        Self {
            score,
            rooms: game.rooms_cleared,
            duration_ms,
            finished_at_unix,
            seeded: game.seeded_run,
        }
    }

    fn finished_at_str(&self) -> String {
        Local
            .timestamp_opt(self.finished_at_unix, 0)
            .single()
            .map_or_else(|| "-".to_string(), |dt| dt.format("%Y/%m/%d").to_string())
    }

    fn duration_str(&self) -> String {
        format_duration(Duration::from_millis(self.duration_ms))
    }
}

#[derive(Serialize, Deserialize, Default)]
struct ScoreFile {
    entries: Vec<ScoreEntry>,
}

struct ScoreStore {
    path: PathBuf,
    file: ScoreFile,
}

impl ScoreStore {
    fn new() -> anyhow::Result<Self> {
        let proj = ProjectDirs::from("", "", "scoundrel")
            .ok_or_else(|| anyhow::anyhow!("Could not resolve data dir"))?;

        let dir = proj.data_local_dir().to_path_buf();
        fs::create_dir_all(&dir)?;

        let path = dir.join("scores.json");
        let file = if path.exists() {
            let bytes = fs::read(&path)?;
            serde_json::from_slice::<ScoreFile>(&bytes).unwrap_or_default()
        } else {
            ScoreFile::default()
        };

        Ok(Self { path, file })
    }

    fn entries(&self) -> &[ScoreEntry] {
        &self.file.entries
    }

    fn add(&mut self, entry: ScoreEntry) -> anyhow::Result<()> {
        self.file.entries.push(entry);

        // sort: score desc, room asc, duration asc, date desc (most recent first)
        self.file.entries.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.rooms.cmp(&b.rooms))
                .then_with(|| a.duration_ms.cmp(&b.duration_ms))
                .then_with(|| b.finished_at_unix.cmp(&a.finished_at_unix))
        });

        self.file.entries.truncate(MAX_SCORES);
        self.save()
    }

    fn clear(&mut self) -> anyhow::Result<()> {
        self.file.entries.clear();
        self.save()
    }

    fn save(&self) -> anyhow::Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.file)?;
        fs::write(&tmp, bytes)?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

impl Widget for &ScoreStore {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let entries = self.entries();

        let header = Row::new([
            Cell::from("#"),
            Cell::from("Score"),
            Cell::from("Rooms"),
            Cell::from("Time"),
            Cell::from("Seed"),
            Cell::from("Date"),
        ])
        .style(Styling::Bold);

        let rows = entries.iter().enumerate().map(|(i, e)| {
            let marker = if e.seeded { "seed" } else { "" };

            Row::new([
                Cell::from((i + 1).to_string()),
                Cell::from(e.score.to_string()),
                Cell::from(e.rooms.to_string()),
                Cell::from(e.duration_str()),
                Cell::from(marker),
                Cell::from(e.finished_at_str()),
            ])
        });

        let widths = [
            Constraint::Length(2),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Fill(1),
        ];

        let table_area = centered_width(area, MENU_SCORES_WIDTH);
        let table_title = "Scores (Top 10)";

        if entries.is_empty() {
            let block = Styling::block(table_title);
            let inner = block.inner(table_area);

            block.render(table_area, buf);
            buf.set_style(inner, Styling::Dim);

            for y in inner.top()..inner.bottom() {
                for x in inner.left()..inner.right() {
                    buf.cell_mut((x, y)).unwrap().set_char('/');
                }
            }

            let [_, center, _] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .areas(inner);

            Paragraph::new(" No scores yet. ")
                .alignment(HorizontalAlignment::Center)
                .render(center, buf);

            return;
        }

        Table::new(rows, widths)
            .header(header)
            .block(Styling::block(table_title).padding(Padding::uniform(1)))
            .render(table_area, buf);
    }
}

enum Scene {
    Menu,
    Game,
}

enum MenuPage {
    Home(usize),
    Seed,
    Scores,
    ClearConfirm(bool),
}

impl MenuPage {
    fn idx(&self) -> usize {
        match self {
            Self::Home(_) => 0,
            Self::Seed => 1,
            Self::Scores => 2,
            Self::ClearConfirm(_) => 3,
        }
    }
}

#[derive(Clone, Copy)]
enum MenuItem {
    StartRandom,
    StartSeeded,
    ViewScores,
    ClearScores,
}

impl MenuItem {
    const ALL: [MenuItem; 4] = [
        Self::StartRandom,
        Self::StartSeeded,
        Self::ViewScores,
        Self::ClearScores,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::StartRandom => "Start Game",
            Self::StartSeeded => "Choose Seed",
            Self::ViewScores => "Scores",
            Self::ClearScores => "Reset",
        }
    }
}

struct SeedTextarea {
    textarea: TextArea<'static>,
    error: Option<String>,
}

impl SeedTextarea {
    fn new() -> Self {
        let mut ta = TextArea::default();
        ta.set_cursor_line_style(Styling::Default.into());
        ta.set_placeholder_text("e.g. 12345");
        ta.set_block(Styling::block("Enter seed"));

        Self {
            textarea: ta,
            error: None,
        }
    }

    fn current_seed_str(&self) -> String {
        self.textarea
            .lines()
            .first()
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }
}

impl Widget for &SeedTextarea {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [ta_area, _] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(MENU_SEED_HEIGHT), Constraint::Fill(1)])
            .margin(1)
            .areas(centered_width(area, MENU_SEED_WIDTH));

        self.textarea.clone().render(ta_area, buf);
    }
}

struct MenuHeader {
    logo: Text<'static>,
    logo_w: u16,
    logo_h: u16,
    version_line: Line<'static>,
}

impl MenuHeader {
    fn new() -> Self {
        let lines: Vec<Line<'static>> = LOGO
            .trim_matches('\n')
            .lines()
            .map(|s| Line::from(s.to_string()))
            .collect();

        let logo_h = u16::try_from(lines.len()).expect("Logo too tall");
        let logo_w = u16::try_from(
            LOGO.trim_matches('\n')
                .lines()
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0),
        )
        .expect("Logo too wide");

        let version_line = Line::from(Span::styled(format!("v{}", VERSION), Styling::Bold));

        Self {
            logo: Text::from(lines),
            logo_w,
            logo_h,
            version_line,
        }
    }
}

impl Widget for &MenuHeader {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.logo_w > area.width || self.logo_h + 2 > area.height {
            let logo_area = centered_width(area, GAME_MAX_WIDTH);
            Line::from(vec![
                Span::styled("Scoundrel ", Styling::Bold),
                Span::styled(format!("v{}", VERSION), Styling::Dim),
            ])
            .left_aligned()
            .render(logo_area, buf);
            return;
        }

        let header_area_clamped = area.clamp(Rect {
            width: self.logo_w,
            x: area.x + area.width.saturating_sub(self.logo_w) / 2,
            ..area
        });

        let [logo_area, version_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(self.logo_h), Constraint::Length(1)])
            .spacing(1)
            .areas(header_area_clamped);

        Paragraph::new(self.logo.clone())
            .alignment(HorizontalAlignment::Center)
            .style(Styling::Bold)
            .render(logo_area, buf);

        Paragraph::new(self.version_line.clone())
            .alignment(HorizontalAlignment::Right)
            .render(version_area, buf);
    }
}

#[derive(Clone, Copy)]
enum Styling {
    Default,
    Dim,
    Bold,
    Reversed,
}

impl Styling {
    pub fn card_block() -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Self::Default)
    }

    pub fn card_block_dimmed() -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Self::Dim)
    }

    pub fn block(title: impl Into<String>) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Self::Default)
            .title(Span::styled(format!(" {} ", title.into()), Self::Bold))
    }

    pub fn confirm_label(confirmed: bool) -> Self {
        if confirmed {
            Self::Reversed
        } else {
            Self::Default
        }
    }

    pub fn label(enabled: bool) -> Self {
        if enabled { Self::Default } else { Self::Dim }
    }
}

impl From<Styling> for Style {
    fn from(val: Styling) -> Self {
        match val {
            Styling::Default => Style::default(),
            Styling::Dim => Style::default().dim(),
            Styling::Bold => Style::default().bold(),
            Styling::Reversed => Style::default().reversed(),
        }
    }
}

struct App {
    running: bool,
    scene: Scene,

    // menu scene
    menu_page: MenuPage,
    menu_header: MenuHeader,
    seed_textarea: SeedTextarea,

    // game scene
    game: Option<Game>,
    log: MessageLog,
    scores: ScoreStore,
}

impl Default for App {
    fn default() -> Self {
        let scores = ScoreStore::new().unwrap_or_else(|_| ScoreStore {
            path: PathBuf::from("scores.json"),
            file: ScoreFile::default(),
        });

        Self {
            running: true,
            scene: Scene::Menu,
            menu_page: MenuPage::Home(0),
            menu_header: MenuHeader::new(),
            seed_textarea: SeedTextarea::new(),
            game: None,
            log: MessageLog::new(MESSAGE_CAP),
            scores,
        }
    }
}

impl App {
    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        while self.running {
            terminal.draw(|frame| {
                let area = frame.area();
                let buf = frame.buffer_mut();
                self.render(area, buf);
            })?;
            if let Some(key) = handle_keypress()? {
                self.on_key(key);
            }
        }
        Ok(())
    }

    fn start_game_with_seed(&mut self, seed: u64, seeded_run: bool) {
        let mut game = Game::new(seed, seeded_run);
        game.refill_room();
        self.log.clear();
        self.game = Some(game);
        self.scene = Scene::Game;
    }

    fn start_new_game(&mut self) {
        let seed = rand::rng().random();
        self.start_game_with_seed(seed, false);
        self.ingest_events(vec![GameEvent::log("May the fates be kind.")]);
    }

    fn back_to_menu(&mut self) {
        self.scene = Scene::Menu;
        self.menu_page = MenuPage::Home(0);
        self.game = None;
        self.seed_textarea.error = None;
    }

    fn copy_seed_value_to_clipboard(&mut self, seed: u64) {
        let seed = seed.to_string();
        match Clipboard::new().and_then(|mut cb| cb.set_text(seed)) {
            Ok(()) => self.log.log("Copied seed to clipboard."),
            Err(err) => self.log.warn(format!("Failed to copy seed: {}", err)),
        }
    }

    fn copy_seed_to_clipboard(&mut self) {
        let Some(game) = &self.game else {
            self.log.warn("No game loaded.");
            return;
        };

        self.copy_seed_value_to_clipboard(game.seed);
    }

    fn ingest_events(&mut self, events: Vec<GameEvent>) {
        for e in events {
            match e {
                GameEvent::Log(s) => self.log.log(s),
                GameEvent::Warn(s) => self.log.warn(s),
                GameEvent::GameOver { score } => {
                    if let Some(game) = &self.game {
                        let entry = ScoreEntry::from_game(game, score);
                        if let Err(err) = self.scores.add(entry) {
                            self.log.warn(format!("Failed to save score: {}", err));
                        }
                    }
                }
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('q' | 'Q'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.running = false;
            return;
        }

        match self.scene {
            Scene::Menu => self.on_key_menu(key),
            Scene::Game => self.on_key_game(key),
        }
    }

    fn on_key_menu(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc) {
            if let MenuPage::Home(_) = self.menu_page {
                /* no-op */
            } else {
                self.menu_page = MenuPage::Home(self.menu_page.idx());
                self.seed_textarea.error = None;
            }
            return;
        }

        match self.menu_page {
            MenuPage::Home(idx) => match key.code {
                KeyCode::Up | KeyCode::Char('k' | 'K') => {
                    self.menu_page = MenuPage::Home(idx.saturating_sub(1));
                }
                KeyCode::Down | KeyCode::Char('j' | 'J') => {
                    self.menu_page = MenuPage::Home((idx + 1).min(MenuItem::ALL.len() - 1));
                }
                KeyCode::Enter => match MenuItem::ALL[idx] {
                    MenuItem::StartRandom => self.start_new_game(),
                    MenuItem::StartSeeded => {
                        self.seed_textarea = SeedTextarea::new();
                        self.menu_page = MenuPage::Seed;
                    }
                    MenuItem::ViewScores => {
                        self.menu_page = MenuPage::Scores;
                    }
                    MenuItem::ClearScores => {
                        self.menu_page = MenuPage::ClearConfirm(false);
                    }
                },
                _ => {}
            },

            MenuPage::Seed => {
                if key.code == KeyCode::Enter {
                    let s = self.seed_textarea.current_seed_str();
                    match s.parse::<u64>() {
                        Ok(seed) => {
                            self.seed_textarea.error = None;
                            self.start_game_with_seed(seed, true);
                        }
                        Err(_) => {
                            self.seed_textarea.error = Some("Seed must be a valid u64".to_string());
                        }
                    }
                } else {
                    self.seed_textarea.textarea.input(Input::from(key));
                    self.seed_textarea.error = None;
                }
            }
            MenuPage::Scores => { /* no-op */ }

            MenuPage::ClearConfirm(confirmed) => match key.code {
                KeyCode::Left | KeyCode::Char('h' | 'H') => {
                    self.menu_page = MenuPage::ClearConfirm(true);
                }
                KeyCode::Right | KeyCode::Char('l' | 'L') => {
                    self.menu_page = MenuPage::ClearConfirm(false);
                }
                KeyCode::Char('y' | 'Y') => self.menu_page = MenuPage::ClearConfirm(true),
                KeyCode::Char('n' | 'N') => self.menu_page = MenuPage::ClearConfirm(false),
                KeyCode::Enter => {
                    if confirmed {
                        if let Err(err) = self.scores.clear() {
                            self.log.warn(format!("Failed to clear scores: {}", err));
                        } else {
                            self.log.log("Scores cleared.");
                        }
                    }
                    self.menu_page = MenuPage::Home(0);
                }
                _ => {}
            },
        }
    }

    fn on_key_game(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc) {
            self.back_to_menu();
            return;
        }

        let Some(game) = self.game.as_mut() else {
            return;
        };

        match game.phase {
            Phase::Dungeon => {
                if let Some(action) = map_key_to_dungeon_action(key) {
                    let events = game.dispatch(action);
                    self.ingest_events(events);
                }
            }
            Phase::GameOver => match key.code {
                KeyCode::Enter => self.start_new_game(),
                KeyCode::Char('c' | 'C') => self.copy_seed_to_clipboard(),
                _ => {}
            },
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        if area.width < MIN_TERM_WIDTH || area.height < MIN_TERM_HEIGHT {
            self.render_too_small(area, buf);
            return;
        }

        let [page_area, footer_area] = Layout::default()
            .constraints([Constraint::Fill(1), Constraint::Length(1)])
            .areas(area);

        match self.scene {
            Scene::Menu => self.render_menu(page_area, buf),
            Scene::Game => self.render_game(page_area, buf),
        }

        self.render_footer(footer_area, buf);
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let text = match self.scene {
            Scene::Menu => match self.menu_page {
                MenuPage::Home(_) => "Enter: select   Up/Down navigate   Ctrl+Q: quit",
                MenuPage::Seed => "Enter: start   Esc: back   Ctrl+Q: quit",
                MenuPage::Scores => "Esc: back   Ctrl+Q: quit",
                MenuPage::ClearConfirm(_) => "Enter: confirm   Esc: back   Ctrl+Q: quit",
            },
            Scene::Game => match &self.game {
                Some(g) => match g.phase {
                    Phase::Dungeon => {
                        "1-4: select   A: avoid   Space: switch stance   Esc: menu   Ctrl+Q: quit"
                    }
                    Phase::GameOver => "Enter: restart  C: copy seed   Esc: menu   Ctrl+Q: quit",
                },
                None => "Esc: menu   Ctrl+Q: quit",
            },
        };

        Paragraph::new(text)
            .style(Styling::Dim)
            .centered()
            .render(area, buf);
    }

    fn render_menu(&self, area: Rect, buf: &mut Buffer) {
        let constraints =
            if self.menu_header.logo_w > area.width || self.menu_header.logo_h + 2 > area.height {
                vec![Constraint::Length(1), Constraint::Min(0)]
            } else {
                vec![
                    Constraint::Min(self.menu_header.logo_h),
                    Constraint::Length(MENU_BODY_HEIGHT),
                ]
            };

        let [header_area, rest_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .spacing(1)
            .areas(area);

        self.menu_header.render(header_area, buf);

        let [body_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(MENU_BODY_HEIGHT)])
            .flex(Flex::End)
            .areas(rest_area);

        match self.menu_page {
            MenuPage::Home(selected) => self.render_menu_home(body_area, buf, selected),
            MenuPage::Seed => self.seed_textarea.render(body_area, buf),
            MenuPage::Scores => self.scores.render(body_area, buf),
            MenuPage::ClearConfirm(confirmed) => {
                self.render_menu_clear_confirm(body_area, buf, confirmed);
            }
        }
    }

    fn render_menu_home(&self, area: Rect, buf: &mut Buffer, selected: usize) {
        let mut state = ListState::default();
        state.select(Some(selected));

        let items: Vec<ListItem> = MenuItem::ALL
            .iter()
            .copied()
            .map(|it| ListItem::new(Line::from(it.title())))
            .collect();

        let list = List::new(items).highlight_symbol("▶ ");
        let list_area = centered_width(area, MENU_HOME_WIDTH);

        ratatui::widgets::StatefulWidget::render(list, list_area, buf, &mut state);
    }

    fn render_menu_clear_confirm(&self, area: Rect, buf: &mut Buffer, confirmed: bool) {
        let [panel_area, _] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(MENU_CLEAR_CONFIRM_HEIGHT),
                Constraint::Fill(1),
            ])
            .areas(centered_width(area, MENU_CLEAR_CONFIRM_WIDTH));

        let block = Styling::block("Clear all scores?");
        let inner = block.inner(panel_area);
        block.render(panel_area, buf);

        let [message_area, choice_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(1)])
            .vertical_margin(1)
            .areas(inner);

        Paragraph::new("This action cannot be undone.")
            .alignment(HorizontalAlignment::Center)
            .render(message_area, buf);

        Paragraph::new(Line::from(vec![
            Span::styled(" Yes ", Styling::confirm_label(confirmed)),
            Span::raw("   "),
            Span::styled(" No ", Styling::confirm_label(!confirmed)),
        ]))
        .alignment(HorizontalAlignment::Center)
        .render(choice_area, buf);
    }

    fn render_game(&mut self, area: Rect, buf: &mut Buffer) {
        let Some(game) = &self.game else {
            Paragraph::new("No game loaded").render(area, buf);
            return;
        };

        let game_area = centered_width(area, GAME_MAX_WIDTH);
        let [header_area, room_area, weapon_area, bottom_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(CARD_HEIGHT + 1),
                Constraint::Length(CARD_HEIGHT + 1),
                Constraint::Min(7),
            ])
            .spacing(1)
            .areas(game_area);

        let [message_area, status_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(1), Constraint::Length(18)])
            .spacing(1)
            .areas(bottom_area);

        self.render_game_header(header_area, buf, game);
        game.room.render(room_area, buf);
        self.render_game_loadout(weapon_area, buf, game);
        self.log.render(message_area, buf);
        self.render_game_status(status_area, buf, game);
    }

    fn render_game_header(&self, area: Rect, buf: &mut Buffer, game: &Game) {
        let [title_area, time_area, seed_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(10),
                Constraint::Fill(1),
            ])
            .areas(area);

        Line::from(vec![
            Span::styled("Scoundrel ", Styling::Bold),
            Span::styled(format!("v{}", VERSION), Styling::Dim),
        ])
        .left_aligned()
        .render(title_area, buf);

        Line::from(Span::styled(format_duration(game.elapsed()), Styling::Dim))
            .centered()
            .render(time_area, buf);

        Line::from(Span::styled(format!("{}", game.seed), Styling::Dim))
            .right_aligned()
            .render(seed_area, buf);
    }

    fn render_game_loadout(&self, area: Rect, buf: &mut Buffer, game: &Game) {
        let [weapon_area, slain_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(CARD_WIDTH); 2])
            .spacing(CARD_SPACING)
            .flex(Flex::Center)
            .areas(area);

        let [weapon_card_area, weapon_label_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(CARD_HEIGHT), Constraint::Length(1)])
            .areas(weapon_area);

        let [slain_card_area, slain_label_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(CARD_HEIGHT), Constraint::Length(1)])
            .areas(slain_area);

        let weapon_stance = game.player.weapon_stance;

        match &game.player.weapon {
            Some(card) => {
                if weapon_stance {
                    card.render(weapon_card_area, buf);
                } else {
                    card.dimmed().render(weapon_card_area, buf);
                }
            }
            None => EmptyCard.render(weapon_card_area, buf),
        }
        let weapon_label = Span::styled("Weapon", Styling::label(weapon_stance));
        Line::from(weapon_label)
            .centered()
            .render(weapon_label_area, buf);

        match game.player.get_smallest_slain_card() {
            Some(card) => {
                if weapon_stance {
                    card.render(slain_card_area, buf);
                } else {
                    card.dimmed().render(slain_card_area, buf);
                }
            }
            None => EmptyCard.render(slain_card_area, buf),
        }
        let slain_label = Span::styled("Slain", Styling::label(weapon_stance));
        Line::from(slain_label)
            .centered()
            .render(slain_label_area, buf);
    }

    fn render_game_status(&self, area: Rect, buf: &mut Buffer, game: &Game) {
        let mut text = vec![
            Line::from(format!("HP: {}/{}", game.player.health, MAX_HEALTH)),
            Line::from(format!("Dungeon: {}", game.deck.len())),
            Line::from(format!("Room: {}", game.rooms_cleared)),
            Line::from(format!(
                "Stance: {}",
                if game.player.weapon_stance {
                    "Weapon"
                } else {
                    "Fists"
                }
            )),
        ];
        if !game.avoid_available {
            text.push(Line::from("Avoided"));
        }
        if !game.heal_available {
            text.push(Line::from("Healed"));
        }

        Paragraph::new(text)
            .block(Styling::block("Status"))
            .render(area, buf);
    }

    fn render_too_small(&self, area: Rect, buf: &mut Buffer) {
        let msg = vec![
            Line::from("Terminal window too small"),
            Line::from(""),
            Line::from(format!(
                "Minimum required: {}x{} (width x height)",
                MIN_TERM_WIDTH, MIN_TERM_HEIGHT
            )),
            Line::from(format!("Current: {}x{}", area.width, area.height)),
        ];

        Paragraph::new(msg)
            .alignment(HorizontalAlignment::Center)
            .style(Styling::Bold)
            .render(area, buf);
    }
}

fn handle_keypress() -> anyhow::Result<Option<KeyEvent>> {
    let keypress = if event::poll(Duration::from_millis(500))? {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => Some(key),
            _ => None,
        }
    } else {
        None
    };
    Ok(keypress)
}

fn map_key_to_dungeon_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('1' | 'h' | 'H') => Some(Action::SelectSlot(Slot::S1)),
        KeyCode::Char('2' | 'j' | 'J') => Some(Action::SelectSlot(Slot::S2)),
        KeyCode::Char('3' | 'k' | 'K') => Some(Action::SelectSlot(Slot::S3)),
        KeyCode::Char('4' | 'l' | 'L') => Some(Action::SelectSlot(Slot::S4)),
        KeyCode::Char('a' | 'A') => Some(Action::AvoidRoom),
        KeyCode::Char(' ') => Some(Action::ToggleWeaponUsage),
        _ => None,
    }
}

fn centered_width(area: Rect, width: u16) -> Rect {
    let w = width.min(area.width);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(w),
            Constraint::Fill(1),
        ])
        .split(area)[1]
}

fn centered_str(s: &str, width: usize) -> String {
    let len = s.chars().count();
    let total_pad = width.saturating_sub(len);
    let left = total_pad / 2;
    let right = total_pad - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

fn wrap_to_width(s: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }

    let mut out = Vec::new();
    for raw_line in s.split('\n') {
        let mut cur = String::new();
        for word in raw_line.split_whitespace() {
            let extra = if cur.is_empty() {
                word.len()
            } else {
                1 + word.len()
            };
            if cur.len().saturating_add(extra) > width {
                if !cur.is_empty() {
                    out.push(cur);
                    cur = String::new();
                }
                // If a single word is longer than width, hard-split it
                if word.len() > width {
                    let mut rest = word;
                    while rest.len() > width {
                        out.push(rest[..width].to_string());
                        rest = &rest[width..];
                    }
                    if !rest.is_empty() {
                        cur.push_str(rest);
                    }
                } else {
                    cur.push_str(word);
                }
            } else {
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(word);
            }
        }
        out.push(cur);
    }
    out
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;

    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

fn main() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut terminal = ratatui::init();
    app.run(&mut terminal)?;
    ratatui::restore();
    Ok(())
}
