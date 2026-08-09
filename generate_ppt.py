# generate_ppt.py - Main script to generate the IGRIS presentation
import sys
sys.stdout.reconfigure(encoding='utf-8')

from pptx import Presentation
from pptx.util import Inches

# Global presentation reference for helper functions
prs = Presentation()
prs.slide_width = Inches(13.333)
prs.slide_height = Inches(7.5)

from slides_content import (
    slide_title, slide_problem, slide_scope, slide_literature,
    slide_architecture, slide_workflow, slide_features,
    slide_conclusion, slide_references
)

slide_title(prs)
slide_problem(prs)
slide_scope(prs)
slide_literature(prs)
slide_architecture(prs)
slide_workflow(prs)
slide_features(prs)
slide_conclusion(prs)
slide_references(prs)

output_path = r"D:\ecosystem\igrisv4\IGRIS_Presentation.pptx"
prs.save(output_path)
print(f"Presentation saved to: {output_path}")
print(f"Total slides: {len(prs.slides)}")
