<?xml version="1.0" encoding="UTF-8"?>
<!-- The LoD2 building that real CityGML exporters write and the CityGML
     schema does not forbid: the *only* LoD 2 geometry in the file is the
     geometry of the thematic boundary surfaces themselves. There is no
     bldg:lod2Solid and no bldg:lod2MultiSurface on the Building, so the
     polygons under bldg:boundedBy are all there is.

     CityJSON has no place for a boundary surface that is not part of a
     geometry, so the LoD 2 boundary polygons are gathered into a
     MultiSurface of the Building's own — which is exactly what
     citygml-tools does with such a file.

     The Building *does* carry a bldg:lod1MultiSurface, and that geometry
     must be left exactly as it is: the gathering happens per LoD, and only
     where the object has nothing at that LoD.

     A 4 x 3 x 2 box's footprint, south wall (with one window), and flat
     roof, with the lower corner at (1000, 2000, 0). Four LoD 2 polygons,
     four semantic surfaces, and the window is linked to the wall it opens
     from both ends. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:gml="http://www.opengis.net/gml">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>1000 2000 0</gml:lowerCorner>
      <gml:upperCorner>1004 2003 2</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>
  <core:cityObjectMember>
    <bldg:Building gml:id="b1">
      <!-- LoD 1: geometry of the Building's own, which nothing may touch. -->
      <bldg:lod1MultiSurface>
        <gml:MultiSurface>
          <gml:surfaceMember>
            <gml:Polygon gml:id="footprint-lod1">
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>1000 2000 0 1004 2000 0 1004 2003 0 1000 2003 0 1000 2000 0</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </bldg:lod1MultiSurface>

      <bldg:boundedBy>
        <bldg:GroundSurface gml:id="gs1">
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="ground">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 0 1004 2000 0 1004 2003 0 1000 2003 0 1000 2000 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
        </bldg:GroundSurface>
      </bldg:boundedBy>

      <bldg:boundedBy>
        <bldg:WallSurface gml:id="ws1">
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="wall-south">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 0 1004 2000 0 1004 2000 2 1000 2000 2 1000 2000 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
          <bldg:opening>
            <bldg:Window gml:id="win1">
              <bldg:lod2MultiSurface>
                <gml:MultiSurface>
                  <gml:surfaceMember>
                    <gml:Polygon gml:id="window">
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>1001 2000 0.5 1002 2000 0.5 1002 2000 1.5 1001 2000 1.5 1001 2000 0.5</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                </gml:MultiSurface>
              </bldg:lod2MultiSurface>
            </bldg:Window>
          </bldg:opening>
        </bldg:WallSurface>
      </bldg:boundedBy>

      <bldg:boundedBy>
        <bldg:RoofSurface gml:id="rs1">
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="roof">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 2 1004 2000 2 1004 2003 2 1000 2003 2 1000 2000 2</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
        </bldg:RoofSurface>
      </bldg:boundedBy>
    </bldg:Building>
  </core:cityObjectMember>
</core:CityModel>
