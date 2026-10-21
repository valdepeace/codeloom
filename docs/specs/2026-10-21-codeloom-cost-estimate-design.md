# Coste estimado de la sesión en el rail

Estado: propuesta del 2026-10-21 (derivada del catálogo de models.dev, que trae
el precio por millón de tokens de cada modelo). Plan:
`docs/plans/2026-10-21-codeloom-cost-estimate.md`.
Relacionado: `2026-10-21-codeloom-models-catalog-design.md`,
`2026-10-21-codeloom-agent-observability-design.md`.

## Intención

CodeLoom se define por usar agentes y subagentes con eficiencia y poder verlos.
El rail ya muestra los tokens de la sesión y los de cada hijo; falta lo que más
importa a quien paga: cuánto cuesta. El catálogo da `cost.input` y `cost.output`
(dólares por millón de tokens) por modelo, y con ellos se estima.

## Objetivos

1. Una estimación en dólares de la sesión abierta: tokens propios más los de los
   hijos de la jerarquía, por el precio del modelo configurado.
2. Verla en el rail junto a los tokens, con el prefijo `≈` para dejar claro que
   es una estimación.
3. Que no aparezca (ni rompa nada) si no se conoce el precio.

## No objetivos

- Contabilidad exacta o facturación: no hay precio de caché, de razonamiento ni
  de imágenes; es una cota orientativa.
- Coste por hijo (todos los hijos usan el modelo del padre, así que el total
  basta) ni histórico entre ejecuciones.
- Convertir a otras monedas.

## Anti-objetivos

- Mostrar un coste inventado: sin precio en el catálogo, no se muestra nada.
- Trabajo en `render` más allá de una multiplicación y un formato.

## Decisiones de diseño (mías, marcadas para revisión)

- `App::model_prices() -> Option<(f64, f64)>`: precios (entrada, salida) por millón
  del modelo actual (`app.model`) buscando en el catálogo el proveedor cuya
  `base_url` coincida con la de `provider_settings` (sin barra final) y dentro
  de él el modelo por `id`. `None` si falta catálogo, proveedor, modelo o alguno
  de los dos precios.
- `App::estimated_cost() -> Option<f64>`: `(input + hijos.input) * pin / 1e6 +
  (output + hijos.output) * pout / 1e6`, con los contadores de sesión ya existentes
  y `hierarchy_tokens()`.
- Formato: `≈ $0.0123` por debajo de 1 dólar (cuatro decimales), `≈ $1.23` hasta
  100, `≈ $123` por encima; `< $0.0001` si es positivo pero menor. Cero tokens: no
  se muestra.
- Se pinta como una línea tenue bajo la de los tokens del rail.

## Qué se deja al implementador

Nombres internos no fijados en el plan y la colocación exacta de la línea.

## Verificación

Pruebas puras de la búsqueda de precios, del cálculo, del formato y del rail
(con y sin precio). Sin red. Validación visual en terminal real: pendiente.

## Nota (2026-10-21)

La estimación basada en tokens × precio del modelo actual quedó **sustituida** por
el libro de consumo (`2026-10-21-codeloom-usage-ledger-design.md`): `estimated_cost`
suma ahora los apuntes guardados, cada uno con su precio congelado.
