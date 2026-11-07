# DPER_Measure — installation Windows x64

Installer le runtime Microsoft Visual C++ v14 x64 fourni par Microsoft avant
de lancer l'installateur. Celui-ci vérifie sa présence et exige les droits
administrateur. Le pilote de votre caméra doit également être installé.

Le package contient l'application, cette notice, la licence et la mire numérique
de démonstration. Les icônes des onglets sont embarquées dans l'application.
Aucune photo de pièce, aucun projet ni rapport utilisateur n'est fourni.

Certaines fonctions demandent des outils externes accessibles dans PATH :

| Fonction | Prérequis externe |
| --- | --- |
| Lire et analyser les PDF | Poppler : `pdftotext`, `pdftocairo`, `pdftoppm` |
| Reconnaissance de texte sur images | Tesseract et données de langues `fra` et `eng` |
| Enregistrement vidéo / conversion | FFmpeg avec encodeur `libx264` |

Ces outils ne sont pas inclus. Les installer séparément depuis leurs sources
officielles. Fermer et rouvrir DPER_Measure après une modification de PATH.
L'absence d'un outil peut rendre la fonction correspondante indisponible.

Après installation, lancer DPER_Measure depuis le menu Démarrer. Utiliser le
parcours guidé et la mire de démonstration pour découvrir les fonctions.
Cette mire est fictive et ne remplace pas un étalon physique.

Pour une mesure physique, conserver un étalonnage indépendant, vérifier les
contours et suivre le protocole de qualification de la méthode. Un tracé
géométrique, une copie ou une valeur nominale du plan ne valide pas une pièce.

Sauvegarder les projets et rapports dans votre dossier Documents, en dehors
du dossier d'installation. La désinstallation se fait depuis les applications
Windows ; conserver une sauvegarde de vos projets avant toute mise à jour.
