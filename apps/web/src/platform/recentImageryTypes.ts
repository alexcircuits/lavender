export interface CloudRange {
  min: number;
  max: number;
}

export interface TimeRange {
  start: string;
  end: string;
}

export type CandidateAvailability = 'present' | 'empty' | 'unknown';
export type CoverageLevel = 'full' | 'partial' | 'unknown';
export type RankReason = 'clear' | 'cloudy' | 'partial' | 'overview';

export interface Granule {
  id: string;
  product: string;
  timeStart: string;
  timeEnd: string;
  cloud: number | null;
  footprint?: Array<[number, number]>;
}

export interface ImageryCandidate {
  key: string;
  product: string;
  day: string;
  cloud: CloudRange | null;
  timeRange: TimeRange | null;
  availability: CandidateAvailability;
  coverage: CoverageLevel;
  granuleCount: number;
  granules?: Granule[];
}

export interface RankedLatest {
  candidate: ImageryCandidate | null;
  reason: RankReason | null;
  certain: boolean;
}

export interface ImageryAttribution {
  credit: string;
  licence: string;
  statement: string;
}

export interface ImageryErrorDetail {
  product: string;
  message: string;
}

export interface RecentImageryResponse {
  candidates: ImageryCandidate[];
  rankedLatest: RankedLatest;
  tileTemplates: Record<string, string>;
  attribution: ImageryAttribution;
  truncated: boolean;
  errors: ImageryErrorDetail[];
}

export interface ThumbnailProbeResponse {
  product: string;
  day: string;
  dataPresent: boolean;
  acquisitionTime?: string | null;
}
