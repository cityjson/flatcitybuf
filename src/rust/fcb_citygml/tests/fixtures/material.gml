<?xml version="1.0" encoding="UTF-8"?>
<!-- The semantic_surfaces.gml building, painted: the same gable-roofed box,
     with its faces stated under bldg:boundedBy and assembled by xlink into a
     lod2Solid, and two appearances over it.

     The two appearances are written in the two different places CityGML
     allows. "summer" is an app:appearanceMember of the CityModel and paints
     the two roof polygons red; "winter" is an app:appearance *inside* the
     Building — the per-object form, which is equally legal — and paints the
     four walls grey. The ground polygon is targeted by neither, so it must
     come out null under both themes.

     The material order in the converted feature follows the themes' document
     order: "summer" is declared first, so its material is index 0. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:app="http://www.opengis.net/citygml/appearance/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>1000 2000 0</gml:lowerCorner>
      <gml:upperCorner>1010 2006 5</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>

  <app:appearanceMember>
    <app:Appearance>
      <app:theme>summer</app:theme>
      <app:surfaceDataMember>
        <app:X3DMaterial>
          <gml:name>roof-red</gml:name>
          <app:ambientIntensity>0.4</app:ambientIntensity>
          <app:diffuseColor>0.9 0.1 0.1</app:diffuseColor>
          <app:isSmooth>false</app:isSmooth>
          <app:target>#roof-south</app:target>
          <app:target>#roof-north</app:target>
        </app:X3DMaterial>
      </app:surfaceDataMember>
    </app:Appearance>
  </app:appearanceMember>

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

      <!-- The per-object appearance: legal CityGML, and read exactly as the
           CityModel-level one above. -->
      <app:appearance>
        <app:Appearance>
          <app:theme>winter</app:theme>
          <app:surfaceDataMember>
            <app:X3DMaterial>
              <gml:name>wall-grey</gml:name>
              <app:diffuseColor>0.6 0.6 0.6</app:diffuseColor>
              <app:transparency>0.0</app:transparency>
              <app:isSmooth>true</app:isSmooth>
              <app:target>#wall-south</app:target>
              <app:target>#wall-east</app:target>
              <app:target>#wall-north</app:target>
              <app:target>#wall-west</app:target>
            </app:X3DMaterial>
          </app:surfaceDataMember>
        </app:Appearance>
      </app:appearance>

      <bldg:boundedBy>
        <bldg:RoofSurface gml:id="rs1">
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
