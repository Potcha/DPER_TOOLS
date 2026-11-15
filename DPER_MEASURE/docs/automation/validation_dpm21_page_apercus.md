# DPM-21 — Examiner la dernière page d’aperçus

Dans **Plan de contrôle et inspection IA**, **Dernière page d’aperçus**
affiche les résultats de la dernière page, que la recherche ait été
lancée dans l’interface ou via MCP. Un résultat réussi mène au brouillon
et à ses preuves. Un refus indique la caractéristique, son occurrence
et le motif. Le compteur indique aussi les recherches en attente.
Les caractéristiques écartées avant la recherche restent consultables
avec leur prochaine action ou le motif qui les écarte.

Le panneau indique quand la page a expiré. L’ouverture d’un aperçu
revérifie son contexte, puis l’enregistrement exige une action explicite.
La page est un état de session. Les données de conformité restent
consultables dans les panneaux dédiés.

## Emplacements Windows

- Dossier de travail `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`
- Exécutable debug `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`
- Client de recette `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\smoke_inspection_mcp.py`
- Dossier des entrées synthétiques `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm20-pr72-mcp-20261114-164052-922\recipe`. Il contient le journal `rpc_evidence_001.json` et, dans `occurrence-checks`, `synthetic_reviewed.dperproj`, `two_occurrences.png` et `two_occurrences.pdf`.
- Journal du premier rejeu `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm21-page-apercus\rpc_evidence_001.json`

## Recette Windows

1. Depuis la branche de la PR ou depuis `dev` après fusion, lancer une instance dédiée sur un port 18779 libre. Utiliser PowerShell.

   ```powershell
   Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
   $env:DPER_MEASURE_MCP_PORT = '18779'
   cargo run --locked
   ```

2. Dans une deuxième fenêtre PowerShell, exécuter le bloc ci-dessous. Il prépare le projet synthétique en ouvrant le projet préexistant et en rejouant la sélection des occurrences et le recalage du protocole DPM-20. Il lance une page limitée à une occurrence.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
@'
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, 'scripts')
from smoke_inspection_mcp import Mcp

recipe = Path('output/dpm20-pr72-mcp-20261114-164052-922/recipe')
calls = {entry['request'].get('id'): entry['request'].get('params') for entry in
         json.loads((recipe / 'rpc_evidence_001.json').read_text(encoding='utf-8'))}
folder = Path('output/dpm21-page-apercus').resolve()
folder.mkdir(parents=True, exist_ok=True)
client = Mcp(folder, 18779)

def context():
    return client.call('get_measurement_context')

for step in (7, 9, 11, 13, 14):
    call = calls[step]
    args = call['arguments'].copy()
    if 'expected_revision' in args:
        args['expected_revision'] = context()['document_revision']
    client.call(call['name'], args)

before = context()
args = dict(capture_id=before['capture_id'], expected_revision=before['document_revision'], limit=1)
first = client.call('preview_control_plan', args)
assert [job['status'] for job in first['jobs']] == ['previewed', 'pending_limit']
assert context()['measurements'] == before['measurements']
if '--continue' in sys.argv:
    args.update(expected_revision=first['document_revision'], cursor=first['next_cursor'])
    second = client.call('preview_control_plan', args)
    assert [job['status'] for job in second['jobs']] == ['previewed']
    assert second['jobs'][0]['occurrence_id'] != first['jobs'][0]['occurrence_id']
    assert second['next_cursor'] is None
    assert context()['measurements'] == before['measurements']
print('Pages MCP verifiees. Journal:', client.log_path)
'@ | python -
```

3. Ouvrir **Plan de contrôle et inspection IA**. La dernière page doit indiquer un aperçu, zéro échec et une recherche en attente. La liste de mesures doit correspondre à celle du projet ouvert.
4. Cliquer **Continuer les aperçus (8 maximum)**. La page affiche une autre occurrence examinable et zéro recherche en attente.
5. Cliquer **Examiner l’aperçu**. Le panneau **Observer les contours de la photo** doit montrer le brouillon de la bonne occurrence, les points retenus et le résidu. Le brouillon affiche un aperçu sans mesure créée. Cliquer **Enregistrer explicitement l’observation** et vérifier qu’une mesure est créée. La page d’aperçus signale alors son expiration.
6. Relancer la recette logicielle ci-dessous pour le cas mixte. Il contient une occurrence refusée par la recherche, une occurrence suivante exploitable et une caractéristique hors de la recherche groupée. Vérifier les libellés, le motif, la reprise et l’ouverture du brouillon.

## Recette logicielle

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
cargo test --locked inspection_observation_ui::tests::
```

Le bloc Python accepte `--continue` pour exercer les deux pages par HTTP
dans une même exécution en remplaçant la dernière ligne par
`'@ | python - --continue`. Le test Rust manipule les contrôles natifs
egui. La recette interactive de la fenêtre Windows est distincte.

## Résultats exécutés

- 21 tests Rust ciblés réussis. Le scénario mixte vérifie par clics egui
  les motifs d’échec et d’exclusion, l’examen de la deuxième occurrence
  en brouillon et l’expiration de la page à la révision suivante.
- Deux pages d’une occurrence exercées par HTTP MCP avec le projet
  synthétique et l’extraction du PDF. Aucune mesure créée pendant les
  aperçus. Hôte de test sans fenêtre arrêté normalement. Journal dans
  `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm21-page-apercus\rpc_evidence_001.json`.
