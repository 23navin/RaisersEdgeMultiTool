// viz/types.ts
//
// Shared contract for the report visualization library. Every viz component
// takes the same props: a processed ResultSet (columns + rows) plus a viz-
// specific config blob from the profile's structure.yaml. The renderer maps
// `visualization.type` → component via VIZ_REGISTRY, exactly like the imports
// MainPanel maps step types to step components.

import type { ReactElement } from "react";
import type { ResultSet } from "../../../types";

export type VizProps = {
  data: ResultSet;
  config?: unknown;        // viz-specific (e.g. table column defs, chart axes)
  title?: string;
};

export type VizComponent = (props: VizProps) => ReactElement;
