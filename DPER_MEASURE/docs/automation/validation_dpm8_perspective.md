# DPM-8 — Diagnostiquer la perspective par plan

Le diagnostic MCP ajuste une homographie et une similitude aux mêmes quatre
ancres. Il compare ensuite leurs résidus sur quatre autres points distribués
dans le quadrilatère, un par zone proche d’un coin dans chaque repère. Le seuil
de 0,25 px utilisé dans cette recette est un seuil d’essai synthétique.

## Emplacements et démarrage sous Windows

- Dépôt de travail `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE`
- Exécutable debug `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\target\debug\dper_measure.exe`
- Client MCP `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\scripts\smoke_inspection_mcp.py`
- Journal du rejeu `O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm8-perspective-replay\rpc_evidence_001.json` lors du premier passage. Les passages suivants incrémentent le numéro.

Depuis la branche qui contient DPM-8, lancer une instance dédiée dans PowerShell.
Le port 18777 doit être libre. Les coordonnées synthétiques sont générées par
le bloc de recette. Aucun PDF, projet ou fichier image n’est requis.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
$env:DPER_MEASURE_MCP_PORT = '18777'
cargo run --locked
```

## Rejeu MCP

Exécuter ce bloc dans une seconde fenêtre PowerShell, avec Python 3 disponible.
Il compare le contexte de mesure et l’état d’inspection avant et après chaque
appel. Fermer ensuite l’instance dédiée.

```powershell
Set-Location 'O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE'
@'
from pathlib import Path
import math
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, 'scripts')
from smoke_inspection_mcp import Mcp

folder = Path('output/dpm8-perspective-replay').resolve()
folder.mkdir(parents=True, exist_ok=True)
client = Mcp(folder, 18777)

def state():
    return (client.call('get_measurement_context'), client.call('get_inspection_state'))

baseline = state()

def request(projective=False):
    def pair(x, y):
        divisor = 1 + 0.002 * x + 0.001 * y if projective else 1
        return {'plan': {'x': x, 'y': y},
                'image': {'x': (2*x+25)/divisor, 'y': (2*y+40)/divisor}}
    return dict(surface_geometry='single_plane', max_residual_px=0.25,
                anchors=[pair(0,0), pair(200,0), pair(200,200), pair(0,200)],
                controls=[pair(40,40), pair(160,40), pair(160,160), pair(40,160)])

def run(payload, expect_error=False):
    result = client.call('inspect_planar_perspective', payload, expect_error=expect_error)
    assert state() == baseline
    if not expect_error:
        assert not result['input_provenance_verified'] and not result['physical_qualification']
    return result

perspective = run(request(projective=True))
assert perspective['homography']['max_px'] < 1e-8
assert perspective['homography']['within_threshold']
assert math.isclose(perspective['similarity']['max_px'], 27.76235096, abs_tol=1e-7)
assert not perspective['similarity']['within_threshold']

similarity = run(request())
for model in ('homography', 'similarity'):
    assert similarity[model]['max_px'] < 1e-8 and similarity[model]['within_threshold']

shifted = request()
shifted['controls'][0]['image']['x'] += 3
result = run(shifted)
for model in ('homography', 'similarity'):
    assert math.isclose(result[model]['max_px'], 3, abs_tol=1e-8)
    assert not result[model]['within_threshold']

run({**request(), 'surface_geometry': 'multiple_depths'}, expect_error=True)
reused = request()
run({**reused, 'controls': reused['anchors']}, expect_error=True)
print('5 scenarios MCP verifies. Journal:', client.log_path)
'@ | python -
```

## Résultats et portée

| Cas | Homographie, maximum | Similitude, maximum |
| --- | --- | --- |
| Perspective synthétique | ≈ 0 px | 27,76235 px |
| Similitude synthétique | ≈ 0 px | 0 px |
| Contrôle décalé | 3 px | 3 px |
| Plusieurs profondeurs | Refus | Refus |
| Ancres réutilisées comme contrôles | Refus | Refus |

Quatre tests Rust ciblés réussis. Cinq scénarios exercés par transport HTTP
MCP sur un hôte dédié sans fenêtre, avec états identiques après chaque appel
et arrêt normal de l’hôte. Preuves locales dans
`O:\TOOLS\Depot\DPER_TOOLS\DPER_MEASURE\output\dpm8-mcp-20261115-085342-131\rpc_evidence_001.json`.

Le diagnostic compare les modèles à partir de points et d’une géométrie
déclarés. Il ne vérifie pas la provenance des points, n’applique aucune
transformation à une capture et n’établit aucune qualification physique.
