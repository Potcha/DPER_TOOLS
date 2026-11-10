# DPM-15 — incertitude d’un diamètre

`calculate_diameter_uncertainty` calcule le diamètre en pixels et en millimètres à partir d’un rayon fourni en pixels natifs et d’une échelle isotrope explicite. Pour `D = 2rs`, les sensibilités au rayon et à une perturbation fractionnaire de l’échelle sont `2s` et `D`. Le moteur combine les contributions au premier ordre selon la matrice fournie et applique le facteur d’élargissement. [JCGM 100:2008, § 5.2](https://www.iso.org/sites/JCGM/GUM/JCGM100/C045315e-html/C045315e_FILES/MAIN_C045315e/05_e.html)

## Recette manuelle

1. Enregistrer le projet ouvert, fermer l’ancienne instance et lancer dans PowerShell.

   ```powershell
   & 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm15-diameter-app\debug\dper_measure.exe'
   ```

2. Demander dans le chat **« Exécute la recette DPM-15 du modèle de diamètre via MCP »**. Aucun PDF ni image n’est requis. La fonction `verify(client)` du script de recette exécute les appels en lecture seule. Sa commande directe avec `--test-port` est réservée à un hôte de test dédié.
3. Vérifier le diamètre de **400 px / 4 mm** dans les quatre réponses et les valeurs ci-dessous. Les onze entrées invalides doivent être refusées. Les états de mesure et d’inspection sont comparés avant et après chaque appel.

| Corrélation rayon/échelle | Incertitude-type combinée (mm) | Incertitude élargie à k = 2 (mm) |
| --- | ---: | ---: |
| 0 | 0,040199502484 | 0,080399004969 |
| +0,5 | 0,042142615011 | 0,084285230023 |
| −0,5 | 0,038157568057 | 0,076315136113 |
| +1 | 0,044 | 0,088 |

### Exemple JSON pour MCP

```json
{
  "radius_px": 200,
  "standard_uncertainty_radius_px": 0.2,
  "radius_source": "Rayon synthétique de la recette",
  "scale": {
    "millimeters_per_pixel": 0.01,
    "relative_standard_uncertainty": 0.01,
    "source": "Échelle synthétique de la recette"
  },
  "coverage_factor": 2,
  "correlation_matrix": [[1, 0], [0, 1]],
  "correlation_justification": "Hypothèse d’indépendance de la recette synthétique"
}
```

L’ordre de la matrice est `radius_px, relative_scale`. Les deux termes hors diagonale prennent le même coefficient pour les variantes corrélées. L’incertitude-type relative `0.01` signifie 1 %. Les contributions de l’exemple valent 0,004 mm pour le rayon et 0,04 mm pour l’échelle.

Les refus couvrent le rayon nul, l’échelle nulle, l’incertitude négative, la source ou la justification vide, le facteur nul, la matrice mal dimensionnée ou asymétrique, l’annulation exacte, un nominal injecté et une qualification injectée dans l’échelle.

## Domaine

Le résultat porte le statut `calculation_only`. Le rayon, son incertitude et l’échelle sont déclarés par l’appelant. Un résidu d’ajustement ne fournit pas automatiquement l’incertitude du rayon. Le modèle suppose une échelle isotrope et une propagation au premier ordre. Les conditions optiques, la qualification des sources, le raccordement à une mesure réelle et la décision de conformité relèvent de vérifications distinctes. Les limites numériques du budget corrélé s’appliquent.

## Résultats exécutés

- `cargo test --locked --bin dper_measure uncertainty::` a réussi 40 tests ciblés, dont 4 nouveaux, avec 408 tests filtrés et zéro échec. La compilation des tests rapporte 30 avertissements de code inutilisé.
- La recette MCP sur l’hôte dédié 18771 a réussi les quatre calculs et les onze refus. Les états de mesure et d’inspection sont restés identiques après chaque appel et l’hôte s’est arrêté normalement. Les preuves locales sont dans `output/dpm15-diameter-mcp-20261110-140218-684/recipe`.
- `cargo build --locked --bin dper_measure` a réussi avec 40 avertissements de code inutilisé. L’exécutable de recette a pour SHA-256 `3F52844B0310A42A406F5E9A1FA0BA7651245D5DDB86C1F08AA8F92BF1F7FBAF`.
