/// Représentation interne d'une page Notion, indépendante de l'API et du moteur de rendu.
use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Page {
    pub title: String,
    /// Emoji servant d'icône à la page.
    pub icon: Option<String>,
    /// Image de couverture (octets bruts : JPEG, PNG…).
    pub cover: Option<Vec<u8>>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// `children` n'est rempli que pour les titres dépliables, imprimés dépliés.
    Heading { level: u8, text: Vec<Span>, children: Vec<Block> },
    /// Paragraphe ; `children` contient les blocs imbriqués (en retrait).
    Paragraph { text: Vec<Span>, children: Vec<Block> },
    /// Bloc dépliable, imprimé déplié : son titre, puis son contenu.
    Toggle { text: Vec<Span>, children: Vec<Block> },
    BulletedItem { text: Vec<Span>, children: Vec<Block> },
    NumberedItem { text: Vec<Span>, children: Vec<Block> },
    /// Colonnes côte à côte, chacune avec sa part de la largeur (entre 0 et 1).
    Columns(Vec<Column>),
    /// Sous-page : seul un lien est imprimé, pas son contenu.
    ChildPage { title: String, icon: Option<String>, url: String },
    /// Base de données intégrée, telle que l'affiche sa vue.
    Database(Database),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub width: f64,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Database {
    /// Titre de la base d'origine, avec son icône éventuelle.
    pub title: String,
    pub icon: Option<String>,
    /// Nom de la vue imprimée (l'onglet dans Notion).
    pub view_name: String,
    pub layout: Layout,
}

/// Une cellule : du texte enrichi (éventuellement des étiquettes colorées).
pub type Cell = Vec<Span>;

#[derive(Debug, Clone, PartialEq)]
pub enum Layout {
    /// Tableau, éventuellement découpé en groupes (vue groupée par une propriété).
    /// `widths` reprend la largeur des colonnes dans Notion (en pixels), utilisée en proportions.
    Table { headers: Vec<String>, widths: Vec<u32>, groups: Vec<RowGroup> },
    /// Calendrier : grille de `weeks` semaines (lundi → dimanche) à partir de `first_day`.
    /// `month` est le mois affiché pour une vue mensuelle (les autres jours sont grisés).
    Calendar {
        first_day: NaiveDate,
        weeks: usize,
        month: Option<u32>,
        today: NaiveDate,
        /// Période affichée, en clair (« semaine du 26/10 au 01/11/2026 »).
        label: String,
        entries: Vec<(NaiveDate, Cell)>,
    },
    /// Galerie : une carte par ligne, le titre en premier puis les propriétés visibles.
    Gallery { cards: Vec<Vec<Cell>> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowGroup {
    /// Étiquette du groupe ; absente pour un tableau non groupé.
    pub label: Option<Cell>,
    pub rows: Vec<Vec<Cell>>,
}

/// Au-delà de ce nombre de colonnes, un tableau ne tient plus lisiblement en A4 portrait.
pub const MAX_PORTRAIT_COLUMNS: usize = 4;

impl Block {
    pub fn children(&self) -> Vec<&Block> {
        match self {
            Block::Heading { children, .. }
            | Block::Paragraph { children, .. }
            | Block::Toggle { children, .. }
            | Block::BulletedItem { children, .. }
            | Block::NumberedItem { children, .. } => children.iter().collect(),
            Block::Columns(columns) => columns.iter().flat_map(|c| &c.blocks).collect(),
            Block::ChildPage { .. } | Block::Database(_) => Vec::new(),
        }
    }

    /// Vrai si ce bloc, ou l'un de ses descendants, satisfait le prédicat.
    pub fn any(&self, predicate: &impl Fn(&Block) -> bool) -> bool {
        predicate(self) || self.children().into_iter().any(|child| child.any(predicate))
    }

    /// Nombre de colonnes du tableau, si ce bloc est une base affichée en tableau.
    pub fn table_width(&self) -> Option<usize> {
        match self {
            Block::Database(Database { layout: Layout::Table { headers, .. }, .. }) => Some(headers.len()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link: Option<String>,
    /// Couleur Notion (« blue », « red »…) : le texte est alors une étiquette colorée.
    pub tag: Option<String>,
}
