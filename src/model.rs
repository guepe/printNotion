/// Représentation interne d'une page Notion, indépendante de l'API et du moteur de rendu.

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// `children` n'est rempli que pour les titres dépliables, imprimés dépliés.
    Heading { level: u8, text: Vec<Span>, children: Vec<Block> },
    /// Paragraphe, ou toggle imprimé déplié (titre puis contenu en retrait).
    Paragraph { text: Vec<Span>, children: Vec<Block> },
    BulletedItem { text: Vec<Span>, children: Vec<Block> },
    NumberedItem { text: Vec<Span>, children: Vec<Block> },
    /// Colonnes côte à côte, chacune avec ses propres blocs.
    Columns(Vec<Vec<Block>>),
    /// Sous-page : seul un lien est imprimé, pas son contenu.
    ChildPage { title: String, url: String },
    /// Base de données intégrée, imprimée comme un tableau (une cellule = des spans).
    Table { title: String, headers: Vec<String>, rows: Vec<Vec<Vec<Span>>> },
}

/// Au-delà de ce nombre de colonnes, un tableau ne tient plus lisiblement en A4 portrait.
pub const MAX_PORTRAIT_COLUMNS: usize = 4;

impl Block {
    pub fn children(&self) -> Vec<&Block> {
        match self {
            Block::Heading { children, .. }
            | Block::Paragraph { children, .. }
            | Block::BulletedItem { children, .. }
            | Block::NumberedItem { children, .. } => children.iter().collect(),
            Block::Columns(columns) => columns.iter().flatten().collect(),
            Block::ChildPage { .. } | Block::Table { .. } => Vec::new(),
        }
    }

    /// Vrai si ce bloc, ou l'un de ses descendants, satisfait le prédicat.
    pub fn any(&self, predicate: &impl Fn(&Block) -> bool) -> bool {
        predicate(self) || self.children().into_iter().any(|child| child.any(predicate))
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
}
