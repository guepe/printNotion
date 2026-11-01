# User stories : printNotion

État : **version 1.0.0 livrée** (tag `v1.0.0`). Toutes les stories ci-dessous sont réalisées.

**Vision**
En tant qu'administrateur IT, je veux imprimer des pages Notion avec une mise en page maîtrisée et fidèle à Notion, afin d'obtenir des documents papier propres, lisibles et datés.

**Hypothèse confirmée :** on imprime des pages isolées, à la demande.

---

## Connexion et génération

**US1 : Connexion à Notion**
En tant qu'utilisateur, je veux que l'outil se connecte à mon espace Notion avec un token d'intégration, afin d'accéder à mes pages sans manipulation manuelle.
_Critères d'acceptation :_
- Le token est lu dans la variable d'environnement `NOTION_TOKEN`, sinon dans le fichier de configuration `~/Library/Application Support/printnotion/config.toml` (`token = "ntn_…"`). Il n'est jamais écrit en dur dans le code, et les fichiers de secrets sont exclus de git.
- Si le token est absent, l'outil explique comment créer une intégration et où placer le token.
- Si le token est invalide ou révoqué, ou si la page n'est pas partagée avec l'intégration, un message clair explique quoi faire (« ••• » > « Connexions » dans Notion).
- L'outil utilise l'API Notion `2026-03-11`, nécessaire pour lire les vues des bases.

**US2 : Générer un PDF d'une page**
En tant qu'utilisateur, je veux lancer une commande avec l'URL ou l'ID d'une page, afin d'obtenir un PDF de son contenu.
_Critères d'acceptation :_
- L'outil accepte toutes les formes d'URL Notion (`notion.so`, `app.notion.com/p/…`, liens avec `?p=`, `?pvs=`, `?source=…`) ainsi que l'ID brut, avec ou sans tirets.
- Titres, paragraphes et listes (à puces et numérotées, imbriquées) sont rendus correctement, y compris le gras, l'italique, le souligné, le barré, le code et les liens. Les liens internes Notion deviennent des liens absolus.
- Les caractères spéciaux saisis dans Notion sont imprimés tels quels : ils ne peuvent pas être interprétés comme des commandes de mise en page.
- Les écritures non latines (japonais, coréen…) s'affichent grâce à des polices de secours.
- Un bloc non géré ne fait pas planter l'outil : il est signalé par un avertissement et ignoré.
- Le PDF est nommé d'après le titre de la page (caractères interdits remplacés). L'option `-o` choisit un fichier ou un dossier de sortie.

**US3 : Mise en page imprimable**
En tant qu'utilisateur, je veux un en-tête et un pied de page sur chaque page du PDF, afin que le document imprimé reste identifiable hors de Notion.
_Critères d'acceptation :_
- Le format est A4.
- L'en-tête porte le titre du document à partir de la page 2 (la page 1 affiche déjà la couverture et le grand titre).
- Le pied de page porte la date d'impression (« Imprimé le 01/11/2026 ») et la numérotation (« page 2/5 »).

**US4 : Impression directe**
En tant qu'utilisateur, je veux pouvoir envoyer le PDF directement à une imprimante, afin d'éviter de l'ouvrir manuellement.
_Critères d'acceptation :_ `--print` envoie le PDF à l'imprimante par défaut, `--printer NOM` à une imprimante nommée (via CUPS, sur macOS et Linux). Une erreur d'impression est affichée clairement.
_Note :_ pas encore testée sur une vraie imprimante.

---

## Contenu des pages

**US5 : Blocs de structure**
En tant qu'utilisateur, je veux que les colonnes, les blocs dépliables et les sous-pages apparaissent dans le PDF, afin que les pages structurées (comme un menu de la semaine) ne sortent pas vides.
_Critères d'acceptation :_
- Les colonnes sont rendues côte à côte, avec les proportions de Notion, tant que leur contenu y tient. Sinon, elles sont empilées pour que chaque base ait toute la largeur.
- Les toggles et titres dépliables sont imprimés dépliés : leur titre en gras, puis leur contenu en retrait le long d'un filet.
- Les toggles qui servent d'archive sont omis, et leur contenu n'est pas lu. Par défaut : « old », « archive », « archives », « ancien », « anciens ». La liste se règle avec `ignore_toggles = [...]` dans le fichier de configuration.
- Une sous-page apparaît comme une ligne portant son icône, son titre et un lien vers elle, sans imprimer son contenu.

**US6 : Bases de données intégrées**
En tant qu'utilisateur, je veux que les bases intégrées dans une page soient imprimées telles que je les vois dans Notion, afin de retrouver sur papier leur contenu (menus, listes de courses…).
_Critères d'acceptation :_
- L'outil imprime la **première vue (premier onglet)** de chaque base. Pour imprimer un autre onglet, il suffit de le placer en premier dans Notion.
- Le contenu reflète cette vue : mêmes filtres, y compris les filtres rapides, et même tri.
- Les colonnes reprennent celles de la vue, dans le même ordre et avec les mêmes proportions de largeur.
  - Si la vue masque explicitement des colonnes, les colonnes qu'elle ne mentionne pas sont masquées aussi. Sinon, elles sont affichées à la suite, sauf si elles sont vides partout.
  - Les boutons ne sont jamais imprimés.
- Les valeurs sont lisibles :
  - les dates sont au format JJ/MM/AAAA ;
  - les nombres ont au plus 2 décimales et les pourcentages s'écrivent « 92 % » ;
  - les cases à cocher sont dessinées ;
  - les relations affichent le titre des pages liées.
- Chaque base est précédée d'un titre, et la ligne d'en-tête du tableau est répétée sur chaque page.
- Une base inaccessible ne bloque pas le reste. C'est par exemple le cas d'une vue liée dont la base d'origine n'est pas partagée avec l'intégration. L'outil la signale par un avertissement qui explique quoi faire, et compte les bases concernées.

**US8 : Rendu fidèle à Notion**
En tant qu'utilisateur, je veux que le PDF ressemble à ma page Notion, afin de m'y retrouver immédiatement sur papier.
_Critères d'acceptation :_
- **En-tête de page :** la couverture est imprimée en pleine largeur, l'icône de la page la chevauche, puis vient le grand titre.
- **Style :** police Avenir Next, texte gris très foncé, emojis en couleur partout (icônes des pages, des bases et des lignes).
- **Étiquettes :** les sélections, multi-sélections et statuts sont des étiquettes colorées aux couleurs de Notion.
- **Titres des bases :** chaque base porte le nom et l'icône de sa base d'origine, avec l'onglet de la vue en petit.
- **Tableaux :** pas de quadrillage vertical, seulement de fins séparateurs. Un en-tête plus long que le contenu de sa colonne passe sur deux lignes. Une colonne n'est jamais plus étroite que le mot le plus long de son en-tête.
- **Vue groupée :** elle est imprimée par groupes (étiquette colorée et nombre de lignes). Une ligne n'apparaît qu'une fois, même si elle appartient à plusieurs groupes. Les groupes vides sont omis.
- **Vue calendrier :** elle est imprimée comme une grille de la période affichée (semaine ou mois en cours). Les repas apparaissent comme des cartes, les week-ends sont grisés, aujourd'hui est cerclé de rouge, et chaque case prend la hauteur de son contenu.
- **Vue galerie :** elle est imprimée comme des cartes.
- **Textes longs :** comme dans Notion, une vue sans « Retour à la ligne » coupe les textes longs avec « … ».
- **Orientation :** le document passe en paysage s'il contient un tableau de plus de 4 colonnes ou des bases disposées en colonnes.
- **Pagination :** une petite base (15 lignes au plus) n'est jamais coupée entre deux pages.

---

## Utilisation

**US7 : Interface graphique**
En tant qu'utilisateur, je veux coller l'URL d'une page dans une petite fenêtre et obtenir le PDF ouvert directement, afin de ne pas passer par le terminal.
_Critères d'acceptation :_
- Lancé sans argument, l'outil ouvre une fenêtre au style inspiré de Notion. Avec une URL, il reste en ligne de commande.
- Un lien Notion présent dans le presse-papiers est pré-rempli, et un bouton « Coller » permet d'en coller un autre.
- Le PDF est enregistré dans Téléchargements puis ouvert. Les boutons « Ouvrir » et « Afficher dans le Finder » permettent d'y revenir.
- La fenêtre ne se fige pas pendant la génération : une roue tourne dans le bouton.
- Les avertissements sont regroupés dans une section repliable, et les erreurs s'affichent dans une carte rouge.
- Si des bases ne sont pas partagées avec l'intégration, un encadré l'indique, et son bouton « Donner l'accès » ouvre la gestion des intégrations dans Notion.

**US9 : Rapidité**
En tant qu'utilisateur, je veux obtenir mon PDF en quelques secondes, même pour une page riche en bases.
_Critères d'acceptation :_
- Les lignes des bases sont lues par paquets de 100, et non une par une.
- Les blocs, colonnes et bases d'une page sont lus en parallèle, avec 4 requêtes simultanées au maximum pour respecter la limite de débit de Notion. Les réponses « trop de requêtes » sont réessayées.
- Les polices, dont la police emoji, ne sont chargées qu'une fois par lancement.
- Les couvertures Unsplash sont téléchargées en 1600 px plutôt qu'en pleine résolution.
- _Mesure :_ « Menu Semaine » (6 bases, environ 45 appels à l'API) est généré en 7 secondes environ, contre 95 secondes dans la première version.

---

**Limites connues de la V1**
- L'API ne fournit ni l'ordre manuel des groupes (ils suivent l'ordre des options de la base), ni l'onglet affiché à l'écran (l'outil prend le premier), ni l'état replié ou déplié des toggles.
- L'API ne dit pas quelle base d'origine manque quand une vue liée est inaccessible : l'outil ne peut pas la nommer.
- La police Avenir Next, les emojis en couleur et les polices de secours sont celles de macOS. Ailleurs, le PDF retombe sur une police serif, sans emojis en couleur.
- Le binaire n'est pas signé : macOS peut demander une confirmation au premier lancement.

**Hors périmètre V1** _(chacun fera l'objet d'une nouvelle user story)_
- Impression d'une base Notion seule (hors page).
- Contenu des sous-pages.
- Choix d'une autre vue que la première.
- Images, tableaux simples et callouts dans le contenu.
- Application macOS (`.app`) avec icône.
- Version Windows : faisable par compilation croisée (`cargo-xwin`), mais demande d'adapter les polices (Segoe UI), l'impression directe et la fenêtre de console. Écartée pour l'instant, jugée trop lourde.
- Logo ou charte Teconex.
