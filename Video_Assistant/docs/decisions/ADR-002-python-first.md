# ADR 002: noyau Python avant UI distribuee

**Statut**: accepte

**Decision**: implementer le coeur, les schemas et les providers en Python 3.13+,
puis exposer FastAPI. L'UI React consomme uniquement OpenAPI.

**Consequences**: Resolve/FFmpeg/ML partagent le meme processus de coeur et les
meme types. Le contrat JSON permet une UI separee sans couplage technique.

**Alternative retardee**: Python + TypeScript partages par des types genere peut
arriver avec OpenAPI; un schema IDL n'est pas justifie au MVP.

