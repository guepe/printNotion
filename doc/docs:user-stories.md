D'accord, on reste au niveau du besoin, sans code. Voici une première version, qu'on ajustera ensemble.

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

**Hors périmètre V1 :** impression d'une base Notion entière, images, tableaux, toggles, callouts, logo ou charte Teconex. On les ajoutera ensuite, chacun sous forme de nouvelle user story.

**Hypothèse à confirmer :** j'ai supposé qu'en V1 tu imprimes des pages isolées, à la demande.
