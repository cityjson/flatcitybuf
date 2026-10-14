<?xml version="1.0" encoding="UTF-8"?>
<!-- An LoD2 building written the way CityGML writes one: the polygons live
     under the thematic boundary surfaces, and the lod2Solid points at each of
     them with an xlink. The semantics of a face are therefore stated in one
     place (bldg:boundedBy) and the geometry is assembled in another
     (bldg:lod2Solid), and the two must be joined by gml:id.

     The shape is a gable-roofed box: a 10 x 6 footprint with its lower corner
     at (1000, 2000, 0), eaves at z = 3 and a ridge at z = 5 running along x at
     y = 2003. Seven faces: two roof planes, two rectangular eaves walls, two
     pentagonal gable walls, one ground surface. The offset from the origin
     makes a converter that forgets to translate obvious.

     The lod2Solid comes *before* the boundedBy properties on purpose: nothing
     may depend on the boundary surfaces having been written first. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:gen="http://www.opengis.net/citygml/generics/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>1000 2000 0</gml:lowerCorner>
      <gml:upperCorner>1010 2006 5</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>
  <core:cityObjectMember>
    <bldg:Building gml:id="b1">
      <bldg:lod2Solid>
        <gml:Solid>
          <gml:exterior>
            <gml:CompositeSurface>
              <gml:surfaceMember xlink:href="#roof-south"/>
              <gml:surfaceMember xlink:href="#roof-north"/>
              <gml:surfaceMember xlink:href="#wall-south"/>
              <gml:surfaceMember xlink:href="#wall-east"/>
              <gml:surfaceMember xlink:href="#wall-north"/>
              <gml:surfaceMember xlink:href="#wall-west"/>
              <gml:surfaceMember xlink:href="#ground"/>
            </gml:CompositeSurface>
          </gml:exterior>
        </gml:Solid>
      </bldg:lod2Solid>

      <!-- One RoofSurface carrying both roof planes, so that two polygons
           share a single semantic surface. -->
      <bldg:boundedBy>
        <bldg:RoofSurface gml:id="rs1">
          <gen:doubleAttribute name="slope">
            <gen:value>38.7</gen:value>
          </gen:doubleAttribute>
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="roof-south">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 3 1010 2000 3 1010 2003 5 1000 2003 5 1000 2000 3</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <gml:surfaceMember>
                <gml:Polygon gml:id="roof-north">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2003 5 1010 2003 5 1010 2006 3 1000 2006 3 1000 2003 5</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
        </bldg:RoofSurface>
      </bldg:boundedBy>

      <!-- y = 2000, rectangular -->
      <bldg:boundedBy>
        <bldg:WallSurface gml:id="ws-south">
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="wall-south">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 0 1010 2000 0 1010 2000 3 1000 2000 3 1000 2000 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
        </bldg:WallSurface>
      </bldg:boundedBy>

      <!-- x = 1010, the pentagonal gable -->
      <bldg:boundedBy>
        <bldg:WallSurface gml:id="ws-east">
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="wall-east">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1010 2000 0 1010 2006 0 1010 2006 3 1010 2003 5 1010 2000 3 1010 2000 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
        </bldg:WallSurface>
      </bldg:boundedBy>

      <!-- y = 2006, rectangular -->
      <bldg:boundedBy>
        <bldg:WallSurface gml:id="ws-north">
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="wall-north">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1010 2006 0 1000 2006 0 1000 2006 3 1010 2006 3 1010 2006 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
        </bldg:WallSurface>
      </bldg:boundedBy>

      <!-- x = 1000, the other pentagonal gable -->
      <bldg:boundedBy>
        <bldg:WallSurface gml:id="ws-west">
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="wall-west">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2006 0 1000 2000 0 1000 2000 3 1000 2003 5 1000 2006 3 1000 2006 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
        </bldg:WallSurface>
      </bldg:boundedBy>

      <bldg:boundedBy>
        <bldg:GroundSurface gml:id="gs1">
          <gml:name>Footprint</gml:name>
          <bldg:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon gml:id="ground">
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>1000 2000 0 1000 2006 0 1010 2006 0 1010 2000 0 1000 2000 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </bldg:lod2MultiSurface>
        </bldg:GroundSurface>
      </bldg:boundedBy>
    </bldg:Building>
  </core:cityObjectMember>
</core:CityModel>
