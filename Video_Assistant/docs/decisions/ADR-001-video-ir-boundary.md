# ADR 001: le Video IR est la frontiere centrale

**Statut**: accepte

**Decision**: le generateur d'IA produit obligatoirement un Video IR valide. Le
compilateur produit un Edit Plan portable. Les adapters NLE sont interchangeables
et leurs appels ne sortent jamais de `video_assistant.nle`.

**Consequences**: un changement Resolve n'impacte ni le storyboard ni les tests
metier. La couche de primitives reste deterministe, versionnee et teste. Un
nouveau NLE implemente `EditAdapter` et signale ses capacites.

**Alternative rejetee**: demander au LLM un script Resolve par generation. Cette
solution souffrirait d'erreurs silencieuses, de scripts irrepables et d'absence
de validation avant mutation.

