/** True once a client id AND at least one slot is configured - callers use
 * this to decide whether to reserve layout space for ads at all. */
export function adsConfigured(): boolean {
  return Boolean(
    process.env.NEXT_PUBLIC_ADSENSE_CLIENT_ID &&
      (process.env.NEXT_PUBLIC_AD_SLOT_TOP || process.env.NEXT_PUBLIC_AD_SLOT_LEFT || process.env.NEXT_PUBLIC_AD_SLOT_RIGHT)
  );
}

export function sideAdsConfigured(): boolean {
  return Boolean(
    process.env.NEXT_PUBLIC_ADSENSE_CLIENT_ID &&
      (process.env.NEXT_PUBLIC_AD_SLOT_LEFT || process.env.NEXT_PUBLIC_AD_SLOT_RIGHT)
  );
}

export function topAdConfigured(): boolean {
  return Boolean(process.env.NEXT_PUBLIC_ADSENSE_CLIENT_ID && process.env.NEXT_PUBLIC_AD_SLOT_TOP);
}
