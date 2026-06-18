import { getCollection, type CollectionEntry } from "astro:content";

const pageModules = import.meta.glob("../content/pages/**/*.{md,mdx}");

const isPublished = <T extends { data: { draft?: boolean } }>(entry: T) =>
  !entry.data.draft || !import.meta.env.PROD;

export const getPublishedPages = async (): Promise<CollectionEntry<"pages">[]> =>
  Object.keys(pageModules).length > 0
    ? (await getCollection("pages")).filter(isPublished)
    : [];
