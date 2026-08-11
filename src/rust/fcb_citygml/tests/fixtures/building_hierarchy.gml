<?xml version="1.0" encoding="UTF-8"?>
<!-- A building that is nothing but its children: b1 carries one attribute and
     no geometry of its own, and holds two LoD1 BuildingParts and one
     BuildingInstallation.

     The second part deliberately has no gml:id, so the generated
     "{parent}-part-{n}" name is exercised, and it stands 10 m east of the
     first so that the two cubes share no vertex. The installation sits at
     z = 5, above both, which is what fixes the extent's upper z.

     Every coordinate in the feature is pooled into one vertex array: the
     parts' and the installation's indices all address the same list. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>1000 2000 0</gml:lowerCorner>
      <gml:upperCorner>1011 2001 5</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>
  <core:cityObjectMember>
    <bldg:Building gml:id="b1">
      <bldg:measuredHeight uom="m">5.0</bldg:measuredHeight>
      <bldg:consistsOfBuildingPart>
        <bldg:BuildingPart gml:id="p1">
          <bldg:lod1Solid>
            <gml:Solid>
              <gml:exterior>
                <gml:CompositeSurface>
                  <!-- bottom, z = 0 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1000 2000 0 1001 2000 0 1001 2001 0 1000 2001 0 1000 2000 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- top, z = 1 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1000 2000 1 1000 2001 1 1001 2001 1 1001 2000 1 1000 2000 1</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- y = 2000 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1000 2000 0 1000 2000 1 1001 2000 1 1001 2000 0 1000 2000 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- y = 2001 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1001 2001 0 1001 2001 1 1000 2001 1 1000 2001 0 1001 2001 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- x = 1000 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1000 2001 0 1000 2001 1 1000 2000 1 1000 2000 0 1000 2001 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- x = 1001 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1001 2000 0 1001 2000 1 1001 2001 1 1001 2001 0 1001 2000 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                </gml:CompositeSurface>
              </gml:exterior>
            </gml:Solid>
          </bldg:lod1Solid>
        </bldg:BuildingPart>
      </bldg:consistsOfBuildingPart>
      <bldg:consistsOfBuildingPart>
        <!-- No gml:id: this part is named after its parent and its place. -->
        <bldg:BuildingPart>
          <bldg:lod1Solid>
            <gml:Solid>
              <gml:exterior>
                <gml:CompositeSurface>
                  <!-- bottom, z = 0 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1010 2000 0 1011 2000 0 1011 2001 0 1010 2001 0 1010 2000 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- top, z = 1 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1010 2000 1 1010 2001 1 1011 2001 1 1011 2000 1 1010 2000 1</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- y = 2000 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1010 2000 0 1010 2000 1 1011 2000 1 1011 2000 0 1010 2000 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- y = 2001 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1011 2001 0 1011 2001 1 1010 2001 1 1010 2001 0 1011 2001 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- x = 1010 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1010 2001 0 1010 2001 1 1010 2000 1 1010 2000 0 1010 2001 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- x = 1011 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1011 2000 0 1011 2000 1 1011 2001 1 1011 2001 0 1011 2000 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                </gml:CompositeSurface>
              </gml:exterior>
            </gml:Solid>
          </bldg:lod1Solid>
        </bldg:BuildingPart>
      </bldg:consistsOfBuildingPart>
      <bldg:outerBuildingInstallation>
        <bldg:BuildingInstallation gml:id="i1">
          <!-- An installation states its geometry through lodXGeometry, which
               may hold any GML geometry; here it is a one-polygon
               MultiSurface. -->
          <bldg:lod2Geometry>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 5 1001 2000 5 1001 2001 5 1000 2000 5</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2Geometry>
        </bldg:BuildingInstallation>
      </bldg:outerBuildingInstallation>
    </bldg:Building>
  </core:cityObjectMember>
</core:CityModel>
