from types import SimpleNamespace
import unittest
import ezdxf
from SleufBase.cadastral_export import CadastralDxfExporter


class TemplateProfileLayerColorsTests(unittest.TestCase):
    def test_new_layers_use_requested_color_and_profile_lineweight(self):
        doc=ezdxf.new("R2018")
        exporter=CadastralDxfExporter(wfs_client=None)
        exporter._ensure_template_profile_layers(doc,[SimpleNamespace(layer_name="Water",color=3)])
        self.assertEqual(doc.layers.get("Water").dxf.color,3)
        self.assertEqual(doc.layers.get("Water").dxf.lineweight,exporter.TEMPLATE_PROFILE_LINEWEIGHT)

    def test_repeated_colors_and_unicode_names_follow_real_table_keys(self):
        doc=ezdxf.new("R2018");exporter=CadastralDxfExporter(wfs_client=None)
        points=lambda n,c:SimpleNamespace(layer_name=n,color=c)
        exporter._ensure_template_profile_layers(doc,[points("Water",3),points("Straße",1),points("STRASSE",2)])
        exporter._ensure_template_profile_layers(doc,[points("water",5)])
        exporter._ensure_template_profile_layers(doc,[points("Water",3)])
        self.assertEqual(doc.layers.get("water").dxf.color,3)
        self.assertEqual(doc.layers.get("Straße").dxf.color,1)
        self.assertEqual(doc.layers.get("STRASSE").dxf.color,2)


if __name__=="__main__":unittest.main()
