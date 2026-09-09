import { getActiveOutages } from "@/lib/db";
import { NocView } from "@/components/NocView";

export const dynamic = "force-dynamic";

export default async function NocPage() {
  const outages = await getActiveOutages();
  return <NocView outages={outages} />;
}
