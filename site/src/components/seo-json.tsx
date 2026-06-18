/**
 * Consolidated JSON-LD rendering for schema.org structured data.
 *
 * Use this file in two ways:
 * 1. Page-level schemas: pass `jsonLd` to <Layout />. This keeps the schema in
 *    the page shell with the rest of the SEO metadata.
 * 2. Section-level schemas: render <SeoJson schema={...} /> inside the React
 *    section that owns the matching visible content, such as an FAQ section.
 *
 * Keep helpers limited to low-variance schemas this project renders from
 * repeated local UI patterns. For customer-specific schema.org types, use
 * schema-dts directly and pass the object to SeoJson.
 *
 * Example:
 *
 * const faqItems = [
 *   {
 *     question: "Can I use custom schemas?",
 *     answer: "Yes. Pass any schema.org JSON-LD object to SeoJson.",
 *   },
 * ];
 *
 * <SeoJson
 *   schema={createFaqPageSchema(faqItems, {
 *     name: "Product FAQ",
 *     url: "https://example.com/pricing",
 *   })}
 * />
 *
 * For schema.org types without a local helper, create a typed object with
 * WithContext from schema-dts:
 *
 * const serviceJsonLd: WithContext<Service> = {
 *   "@context": "https://schema.org",
 *   "@type": "Service",
 *   name: "Website redesign",
 *   provider: { "@type": "Organization", name: "Example Studio" },
 * };
 *
 * <SeoJson schema={serviceJsonLd} />
 *
 * SeoJson safely serializes the payload, removes duplicate per-node contexts,
 * and combines multiple schemas into one @graph.
 */
import type { FAQPage, Question, WithContext } from "schema-dts";

export type SeoJsonNode = object & {
  "@context"?: "https://schema.org";
  "@type"?: string | readonly string[];
};
export type SeoJsonGraph = object & {
  "@context"?: "https://schema.org";
  "@graph": readonly SeoJsonNode[];
};
export type SeoJsonSchema = SeoJsonNode | SeoJsonGraph;

type SeoJsonInput =
  | SeoJsonSchema
  | readonly SeoJsonSchema[]
  | null
  | false
  | undefined;

interface SeoJsonProps {
  schema?: SeoJsonInput;
  schemas?: SeoJsonInput;
}

export interface FaqItem {
  question: string;
  answer: string;
}

export interface FaqSchemaOptions {
  name?: string;
  description?: string;
  url?: string;
}

export function SeoJson({ schema, schemas }: SeoJsonProps) {
  const nodes = normalizeSchemas(schema, schemas);

  if (nodes.length === 0) return null;

  return (
    <script
      type="application/ld+json"
      dangerouslySetInnerHTML={{ __html: serializeJsonLd(nodes) }}
    />
  );
}

export function createFaqPageSchema(
  items: readonly FaqItem[],
  { name, description, url }: FaqSchemaOptions = {},
): WithContext<FAQPage> {
  return {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    ...(name && { name }),
    ...(description && { description }),
    ...(url && { url }),
    mainEntity: items.map(
      ({ question, answer }): Question => ({
        "@type": "Question",
        name: question,
        acceptedAnswer: {
          "@type": "Answer",
          text: answer,
        },
      }),
    ),
  };
}

function normalizeSchemas(...inputs: readonly SeoJsonInput[]): SeoJsonNode[] {
  const nodes: SeoJsonNode[] = [];

  for (const input of inputs) {
    if (!input) continue;

    const schemas = Array.isArray(input) ? input : [input];

    for (const value of schemas) {
      if (isGraph(value)) {
        nodes.push(...value["@graph"].map(stripContext));
      } else {
        nodes.push(stripContext(value));
      }
    }
  }

  return nodes;
}

function isGraph(value: SeoJsonSchema): value is SeoJsonGraph {
  return "@graph" in value;
}

function stripContext(value: SeoJsonNode): SeoJsonNode {
  const node = { ...(value as Record<string, unknown>) };
  delete node["@context"];

  return node as SeoJsonNode;
}

function serializeJsonLd(nodes: readonly SeoJsonNode[]): string {
  const payload =
    nodes.length === 1
      ? {
          "@context": "https://schema.org",
          ...(nodes[0] as Record<string, unknown>),
        }
      : { "@context": "https://schema.org", "@graph": nodes };

  return JSON.stringify(payload).replace(/</g, "\\u003c");
}
