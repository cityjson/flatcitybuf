<?xml version="1.0" encoding="UTF-8"?>
<!-- Implicit geometry: a prototype written once in its own local coordinate
     system, and placed in the world by a transformation matrix and a
     reference point.

     CityJSON has no implicit geometry, so each placement is flattened: every
     point of the template is run through the 4x4 row-major matrix and then
     translated by the reference point, and what comes out is an ordinary
     geometry of the object that carried the property.

     The two members cover the two ways a placement names its template.

     The tree states its template inline. Its matrix is a diagonal scale of 2,
     so the template triangle (0,0,0) (1,0,0) (0,1,2) doubles to (0,0,0)
     (2,0,0) (0,2,4) and lands at the reference point (100, 200, 5):
     (100,200,5) (102,200,5) (100,202,9).

     The bench places the same template twice, and the second placement reaches
     it by xlink:href — the case a document uses when one prototype serves many
     objects. The LoD 1 matrix is the identity, so its triangle is the template
     itself at (110, 200, 0): (110,200,0) (111,200,0) (110,201,0). The LoD 2
     matrix carries a translation in its fourth column — +5 in x, +1 in z —
     which is applied before the reference point, giving (115,200,1)
     (116,200,1) (115,201,1). A converter that read the matrix column-major, or
     that dropped the fourth column, misses that geometry entirely.

     Only a template inside the same cityObjectMember can be reached this way.
     A document that shares one prototype across members — which is what PLATEAU
     data does — is not resolved here; those placements are skipped and
     reported. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:veg="http://www.opengis.net/citygml/vegetation/2.0"
                xmlns:frn="http://www.opengis.net/citygml/cityfurniture/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>100 200 0</gml:lowerCorner>
      <gml:upperCorner>116 202 9</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>

  <!-- A tree whose crown is a prototype scaled by two and placed at
       (100, 200, 5). -->
  <core:cityObjectMember>
    <veg:SolitaryVegetationObject gml:id="tree-1">
      <gml:name>Linden</gml:name>
      <veg:lod2ImplicitRepresentation>
        <core:ImplicitGeometry>
          <core:transformationMatrix>2 0 0 0 0 2 0 0 0 0 2 0 0 0 0 1</core:transformationMatrix>
          <core:referencePoint>
            <gml:Point>
              <gml:pos>100 200 5</gml:pos>
            </gml:Point>
          </core:referencePoint>
          <core:relativeGMLGeometry>
            <gml:MultiSurface gml:id="tree-template">
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>0 0 0 1 0 0 0 1 2 0 0 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </core:relativeGMLGeometry>
        </core:ImplicitGeometry>
      </veg:lod2ImplicitRepresentation>
    </veg:SolitaryVegetationObject>
  </core:cityObjectMember>

  <!-- A bench placed twice from one template: inline at LoD 1, by reference at
       LoD 2. Both placements are independent geometries of the object, in the
       order the document writes them. -->
  <core:cityObjectMember>
    <frn:CityFurniture gml:id="bench-1">
      <gml:name>Bench</gml:name>
      <frn:lod1ImplicitRepresentation>
        <core:ImplicitGeometry>
          <core:transformationMatrix>1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1</core:transformationMatrix>
          <core:referencePoint>
            <gml:Point>
              <gml:pos>110 200 0</gml:pos>
            </gml:Point>
          </core:referencePoint>
          <core:relativeGMLGeometry>
            <gml:MultiSurface gml:id="bench-template">
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>0 0 0 1 0 0 0 1 0 0 0 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </core:relativeGMLGeometry>
        </core:ImplicitGeometry>
      </frn:lod1ImplicitRepresentation>
      <frn:lod2ImplicitRepresentation>
        <core:ImplicitGeometry>
          <core:transformationMatrix>1 0 0 5 0 1 0 0 0 0 1 1 0 0 0 1</core:transformationMatrix>
          <core:referencePoint>
            <gml:Point>
              <gml:pos>110 200 0</gml:pos>
            </gml:Point>
          </core:referencePoint>
          <core:relativeGMLGeometry xlink:href="#bench-template"/>
        </core:ImplicitGeometry>
      </frn:lod2ImplicitRepresentation>
    </frn:CityFurniture>
  </core:cityObjectMember>
</core:CityModel>
