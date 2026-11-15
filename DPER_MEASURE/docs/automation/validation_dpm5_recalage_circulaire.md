# DPM-5 — Appliquer une hypothèse de recalage circulaire

`propose_image_circle_landmark_matches` recherche des repères sur une
photo figée pour une vue PDF explicite. Une hypothèse unique fournit
une empreinte en plus de ses correspondances et résidus. Après revue,
`register_image_circle_landmarks` reçoit la recherche d’origine et
cette empreinte. L’application recalcule l’appariement sur les
sources courantes avant d’enregistrer la similitude dans la vue.

Une recherche modifiée, une page ou des pixels changés, une
révision périmée ou plusieurs hypothèses compatibles font refuser
l’application. Le recalage reprend les garde-fous et l’invalidation
des preuves de l’opération existante. L’étalonnage physique reste
indépendant. Le domaine de cette recette couvre les disques visibles,
isolés et contrastés et la similitude sur une vue explicite.

## Rejeu sous Windows

- Dossier de travail `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`
- Exécutable debug `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`
- Générateur et client MCP `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\verify_observed_circle_landmarks_mcp.py`
- Jeux d’essai et journal dans un nouveau sous-dossier de `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm5-recalage-circulaire`

Avec Python, Pillow et Poppler, lancer une instance dédiée sur le
port 18782 et exécuter le second bloc depuis une autre fenêtre PowerShell.
Le bloc ouvre deux images synthétiques et affecte un recalage à un projet
de test. Fermer ensuite l’instance dédiée.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
$env:DPER_MEASURE_MCP_PORT = '18782'
cargo run --locked
```

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
@'
from datetime import datetime
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, 'scripts')
from verify_observed_circle_landmarks_mcp import Mcp, make_fixtures, prepare_views

folder = (Path('output/dpm5-recalage-circulaire') /
          datetime.now().strftime('%Y%m%d-%H%M%S-%f')).resolve()
folder.mkdir(parents=True)
pdf, _ = make_fixtures(folder)
client = Mcp(folder, 18782)

def search_for(photo):
    triangle, _ = prepare_views(client, pdf, folder / photo)
    del triangle['image_circle_searches']
    return triangle

def context():
    return (client.call('get_measurement_context'), client.call('get_inspection_state'))

def apply(search, proof, error=None):
    result = client.call('register_image_circle_landmarks',
                         {'search': search, 'proposal_fingerprint': proof},
                         expect_error=error is not None)
    if error:
        assert error in result['content'][0]['text'], result
    return result

search = search_for('photo-reperes-unique.png')
baseline = context()
proposal = client.call('propose_image_circle_landmark_matches', search)
matching = proposal['proposal']['matching']
assert matching['state'] == 'unique_hypothesis' and matching['accepted_count'] == 1
assert context() == baseline
proof = proposal['proposal_fingerprint']
apply(search, 'invalide', 'proposition a changé')
apply(dict(search, threshold_px=0.7), proof, 'proposition a changé')
assert context() == baseline

result = apply(search, proof)
assert result['applied_proposal_fingerprint'] == proof
assert result['document_revision'] == baseline[0]['document_revision'] + 1
registration = result['inspection']['registration']
expected = matching['hypotheses'][0]['registration']
assert registration['correspondences'] == expected['correspondences']
transform = registration['transform']
assert abs(transform['scale'] - 1) < 0.01
translation = transform['translation']
assert abs(translation['x'] - 22) < 0.25 and abs(translation['y'] - 18) < 0.25
assert result['inspection']['registration_scope']['view_id'] == search['view_id']
after = context()
assert all(after[0][key] == baseline[0][key] for key in ('calibration', 'measurements'))
apply(search, proof, 'document a changé')
assert context() == after

project = folder / 'recalage.dperproj'
client.call('save_project', {'path': str(project), 'overwrite': False})
client.call('open_project', {'path': str(project)})
reopened = client.call('get_inspection_state')['inspection']
assert reopened['registration']['correspondences'] == registration['correspondences']
assert reopened['registration_scope'] == result['inspection']['registration_scope']

search = search_for('photo-reperes-ambigus.png')
baseline = context()
ambiguous = client.call('propose_image_circle_landmark_matches', search)
assert ambiguous['proposal']['matching']['state'] == 'ambiguous'
assert 'proposal_fingerprint' not in ambiguous
apply(search, proof, 'hypothèse unique')
assert context() == baseline
print('Recalage, refus et reouverture verifies. Dossier:', folder)
'@ | python -
```

Le cas unique donne une translation voisine de (22, 18) px et une
échelle de recalage voisine de 1. Le cas ambigu reste au stade de
proposition. Les quatre refus vérifient l’empreinte invalide, le
seuil changé, la révision consommée et l’ambiguïté.

## Résultats exécutés

- 25 tests Rust d’appariement et de recalage réussis.
- Recette HTTP MCP réussie sur un hôte dédié sans fenêtre. L’application
  a conservé les correspondances et la vue après réouverture. Les quatre
  refus ont laissé l’état de session identique. Hôte arrêté normalement.
- Journal local dans
  `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm5-recalage-circulaire\20261115-172014-726108\rpc_evidence_001.json`.
