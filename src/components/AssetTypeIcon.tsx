import {
  Archive,
  Box,
  File,
  FileCode2,
  Image,
  Layers3,
  Music2,
  Type,
  Video,
} from "lucide-react";
import type { AssetType } from "../types";

interface Props {
  type: AssetType;
  size?: number;
  strokeWidth?: number;
}

export function AssetTypeIcon({ type, size = 20, strokeWidth = 1.6 }: Props) {
  const props = { size, strokeWidth, "aria-hidden": true };
  switch (type) {
    case "image":
    case "texture":
      return <Image {...props} />;
    case "audio":
      return <Music2 {...props} />;
    case "model":
      return <Box {...props} />;
    case "video":
      return <Video {...props} />;
    case "font":
      return <Type {...props} />;
    case "shader":
      return <FileCode2 {...props} />;
    case "material":
      return <Layers3 {...props} />;
    case "archive":
      return <Archive {...props} />;
    default:
      return <File {...props} />;
  }
}
