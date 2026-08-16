<?xml version="1.0" encoding="UTF-8"?>
<!-- One member of every simple thematic module, in document order: vegetation,
     transportation, water, land use, city furniture, generics and a group.

     The point of the fixture is breadth rather than depth. Each member is the
     smallest thing its module can say — a triangle or a quad, one or two
     attributes — because what is under test is the dispatch, the type each
     element becomes, and the two shapes that are not shared with the building
     module: the traffic areas of a road and the members of a group.

     Everything sits in a 100 x 200 offset from the origin, on the z = 0 plane
     except the city furniture (z = 3) and the generic object (z = 1), so that
     the document's extent is a box rather than a plane and a converter that
     forgets to translate is obvious.

     The road states its geometry before its traffic areas and the water body
     states its boundary surface before its geometry: neither order may
     matter. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:veg="http://www.opengis.net/citygml/vegetation/2.0"
                xmlns:tran="http://www.opengis.net/citygml/transportation/2.0"
                xmlns:wtr="http://www.opengis.net/citygml/waterbody/2.0"
                xmlns:luse="http://www.opengis.net/citygml/landuse/2.0"
                xmlns:frn="http://www.opengis.net/citygml/cityfurniture/2.0"
                xmlns:gen="http://www.opengis.net/citygml/generics/2.0"
                xmlns:grp="http://www.opengis.net/citygml/cityobjectgroup/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>100 200 0</gml:lowerCorner>
      <gml:upperCorner>111 206 3</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>

  <!-- Vegetation: a single tree, whose geometry is a lodXGeometry. -->
  <core:cityObjectMember>
    <veg:SolitaryVegetationObject gml:id="tree-1">
      <gml:name>Linden</gml:name>
      <veg:species>Tilia cordata</veg:species>
      <veg:trunkDiameter uom="m">0.3</veg:trunkDiameter>
      <veg:lod1Geometry>
        <gml:MultiSurface>
          <gml:surfaceMember>
            <gml:Polygon gml:id="tree-p1">
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>100 200 0 101 200 0 101 201 0 100 200 0</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </veg:lod1Geometry>
    </veg:SolitaryVegetationObject>
  </core:cityObjectMember>

  <!-- Vegetation: an area of plants, whose geometry is a lodXMultiSurface. -->
  <core:cityObjectMember>
    <veg:PlantCover gml:id="cover-1">
      <veg:averageHeight uom="m">2.5</veg:averageHeight>
      <veg:lod1MultiSurface>
        <gml:MultiSurface>
          <gml:surfaceMember>
            <gml:Polygon gml:id="cover-p1">
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>102 200 0 103 200 0 103 201 0 102 200 0</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </veg:lod1MultiSurface>
    </veg:PlantCover>
  </core:cityObjectMember>

  <!-- Transportation: a road whose surface is assembled from the polygons of
       its traffic areas, exactly as a building's solid is assembled from the
       polygons of its boundary surfaces. The two traffic areas and the one
       auxiliary traffic area become the semantic surfaces of the road's LoD 2
       geometry, in the order the document writes them. -->
  <core:cityObjectMember>
    <tran:Road gml:id="road-1">
      <gml:name>Main Street</gml:name>
      <tran:class>1000</tran:class>
      <tran:lod2MultiSurface>
        <gml:MultiSurface>
          <gml:surfaceMember xlink:href="#ta-p1"/>
          <gml:surfaceMember xlink:href="#ta-p2"/>
          <gml:surfaceMember xlink:href="#ata-p1"/>
        </gml:MultiSurface>
      </tran:lod2MultiSurface>
      <tran:trafficArea>
        <tran:TrafficArea gml:id="ta-1">
          <gml:name>Carriageway</gml:name>
          <tran:function>1</tran:function>
          <tran:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="ta-p1">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>100 202 0 104 202 0 104 203 0 100 203 0 100 202 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </tran:lod2MultiSurface>
        </tran:TrafficArea>
      </tran:trafficArea>
      <tran:trafficArea>
        <tran:TrafficArea gml:id="ta-2">
          <tran:usage>2</tran:usage>
          <tran:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="ta-p2">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>100 203 0 104 203 0 104 204 0 100 204 0 100 203 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </tran:lod2MultiSurface>
        </tran:TrafficArea>
      </tran:trafficArea>
      <tran:auxiliaryTrafficArea>
        <tran:AuxiliaryTrafficArea gml:id="ata-1">
          <gen:stringAttribute name="surfaceMaterial">
            <gen:value>gravel</gen:value>
          </gen:stringAttribute>
          <tran:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="ata-p1">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>100 204 0 104 204 0 104 204.5 0 100 204.5 0 100 204 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </tran:lod2MultiSurface>
        </tran:AuxiliaryTrafficArea>
      </tran:auxiliaryTrafficArea>
    </tran:Road>
  </core:cityObjectMember>

  <!-- Water: the boundary surface is written first and the geometry points at
       its polygon, which is the other way round from the road. -->
  <core:cityObjectMember>
    <wtr:WaterBody gml:id="water-1">
      <wtr:class>1000</wtr:class>
      <wtr:boundedBy>
        <wtr:WaterSurface gml:id="wsurf-1">
          <gml:name>Lake surface</gml:name>
          <wtr:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="ws-p1">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>105 200 0 107 200 0 107 202 0 105 202 0 105 200 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </wtr:lod2MultiSurface>
        </wtr:WaterSurface>
      </wtr:boundedBy>
      <wtr:lod2MultiSurface>
        <gml:MultiSurface>
          <gml:surfaceMember xlink:href="#ws-p1"/>
        </gml:MultiSurface>
      </wtr:lod2MultiSurface>
    </wtr:WaterBody>
  </core:cityObjectMember>

  <!-- Land use: attributes and one LoD 1 surface, nothing else. -->
  <core:cityObjectMember>
    <luse:LandUse gml:id="lu-1">
      <luse:class>1000</luse:class>
      <luse:lod1MultiSurface>
        <gml:MultiSurface>
          <gml:surfaceMember>
            <gml:Polygon gml:id="lu-p1">
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>100 205 0 102 205 0 102 206 0 100 206 0 100 205 0</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </luse:lod1MultiSurface>
    </luse:LandUse>
  </core:cityObjectMember>

  <!-- City furniture: off the z = 0 plane, so the document's extent has a
       height. -->
  <core:cityObjectMember>
    <frn:CityFurniture gml:id="frn-1">
      <frn:class>1000</frn:class>
      <frn:function>1080</frn:function>
      <frn:lod1Geometry>
        <gml:MultiSurface>
          <gml:surfaceMember>
            <gml:Polygon gml:id="frn-p1">
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>108 200 3 109 200 3 109 201 3 108 200 3</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </frn:lod1Geometry>
    </frn:CityFurniture>
  </core:cityObjectMember>

  <!-- Generics: CityJSON has no type for this, so it becomes the Extension
       type "+GenericCityObject". -->
  <core:cityObjectMember>
    <gen:GenericCityObject gml:id="gen-1">
      <gml:name>Retaining wall</gml:name>
      <gen:class>1000</gen:class>
      <gen:lod1Geometry>
        <gml:MultiSurface>
          <gml:surfaceMember>
            <gml:Polygon gml:id="gen-p1">
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>110 200 1 111 200 1 111 201 1 110 200 1</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </gen:lod1Geometry>
    </gen:GenericCityObject>
  </core:cityObjectMember>

  <!-- A group over two of the members above. Its members are references, and
       they name city objects that are features of their own: a CityJSONSeq
       group refers to them by id across features. The second member states no
       role, which is a null in `children_roles` rather than a shorter array. -->
  <core:cityObjectMember>
    <grp:CityObjectGroup gml:id="group-1">
      <gml:name>Green corridor</gml:name>
      <grp:class>1000</grp:class>
      <grp:groupMember xlink:href="#tree-1" role="part"/>
      <grp:groupMember xlink:href="#road-1"/>
    </grp:CityObjectGroup>
  </core:cityObjectMember>
</core:CityModel>
