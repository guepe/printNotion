# User stories : printNotion

**Vision**
En tant qu'administrateur IT, je veux imprimer des pages Notion avec une mise en page maîtrisée, afin d'obtenir des documents papier propres, lisibles et datés.

**US1 : Connexion à Notion**
En tant qu'utilisateur, je veux que l'outil se connecte à mon espace Notion avec un token d'intégration, afin d'accéder à mes pages sans manipulation manuelle.
_Critères d'acceptation :_ le token est lu depuis une variable d'environnement ou un fichier de config, jamais en dur dans le code. Si le token est absent, invalide, ou si la page n'est pas partagée avec l'intégration, l'outil affiche un message clair qui explique quoi faire.

**US2 : Générer un PDF d'une page**
En tant qu'utilisateur, je veux lancer une commande avec l'URL ou l'ID d'une page, afin d'obtenir un PDF de son contenu.
_Critères d'acceptation :_ les titres, paragraphes et listes (à puces et numérotées) sont rendus correctement, y compris le texte en gras, en italique et les liens. Un bloc non géré ne fait pas planter l'outil : il est signalé par un avertissement et ignoré. Le PDF est nommé d'après le titre de la page.

**US3 : Mise en page imprimable**
En tant qu'utilisateur, je veux un en-tête et un pied de page sur chaque page du PDF, afin que le document imprimé reste identifiable hors de Notion.
_Critères d'acceptation :_ l'en-tête porte le titre du document, le pied de page la date d'impression et la numérotation (« page 2/5 »), et le format est A4.

**US4 : Impression directe** _(optionnelle en V1)_
En tant qu'utilisateur, je veux pouvoir envoyer le PDF directement à une imprimante, afin d'éviter de l'ouvrir manuellement.
_Critères d'acceptation :_ une option de la commande envoie le PDF à l'imprimante par défaut, ou à une imprimante nommée.

**US5 : Blocs de structure**
En tant qu'utilisateur, je veux que les colonnes, les blocs dépliables et les sous-pages apparaissent dans le PDF, afin que les pages structurées (comme un menu de la semaine) ne sortent pas vides.
_Critères d'acceptation :_ les colonnes sont rendues côte à côte, en largeurs égales. Les toggles (et titres dépliables) sont imprimés dépliés : leur titre, puis leur contenu. Une sous-page apparaît comme une ligne portant son titre et un lien vers elle, sans imprimer son contenu.

**US6 : Bases de données intégrées**
En tant qu'utilisateur, je veux que les bases intégrées dans une page soient imprimées sous forme de tableau, afin de retrouver sur papier leur contenu (menus, listes de courses…).
_Critères d'acceptation :_ chaque base est précédée de son titre, avec une ligne d'en-tête répétée sur chaque page. Le tableau reflète la première vue de la base, comme dans Notion : mêmes filtres (y compris les filtres rapides), même tri, mêmes colonnes visibles dans le même ordre. Les valeurs (texte, dates, sélections, cases à cocher, relations…) sont lisibles. Si la vue ne définit pas de colonnes, toutes sont imprimées sauf celles vides partout, et les boutons ne sont jamais imprimés. Si un tableau dépasse 4 colonnes, le document passe en A4 paysage. Une base inaccessible (par exemple une vue liée dont la base d'origine n'est pas partagée) est signalée par un avertissement qui explique quoi faire, sans bloquer le reste.

**US7 : Interface graphique**
En tant qu'utilisateur, je veux coller l'URL d'une page dans une petite fenêtre et obtenir le PDF ouvert directement, afin de ne pas passer par le terminal.
_Critères d'acceptation :_ lancé sans argument, l'outil ouvre une fenêtre ; un lien Notion présent dans le presse-papiers est pré-rempli. Le PDF est enregistré dans Téléchargements puis ouvert ; on peut le rouvrir ou l'afficher dans le Finder. La fenêtre ne se fige pas pendant la génération. Si des bases ne sont pas partagées, un bouton « Donner l'accès » ouvre la gestion des intégrations dans Notion.

**US8 : Rendu fidèle à Notion**
En tant qu'utilisateur, je veux que le PDF ressemble à ma page Notion, afin de m'y retrouver immédiatement sur papier.
_Critères d'acceptation :_ couverture, icône de la page et emojis sont imprimés. Les sélections et statuts sont des étiquettes colorées aux couleurs de Notion. Chaque base porte le nom et l'icône de sa base d'origine, et l'onglet de sa vue. Les colonnes reprennent les largeurs et la visibilité de la vue. Une vue groupée est imprimée par groupes, une vue calendrier comme une grille de la période affichée, une galerie comme des cartes. Les colonnes de la page sont respectées tant que leur contenu y tient. Une page contenant des bases en colonnes passe en paysage.

**Hors périmètre V1 :** impression d'une base Notion seule (hors page), contenu des sous-pages, choix d'une autre vue que la première, ordre manuel des groupes, images dans le contenu, tableaux, callouts, logo ou charte Teconex. On les ajoutera ensuite, chacun sous forme de nouvelle user story.

**Hypothèse à confirmer :** j'ai supposé qu'en V1 tu imprimes des pages isolées, à la demande.
