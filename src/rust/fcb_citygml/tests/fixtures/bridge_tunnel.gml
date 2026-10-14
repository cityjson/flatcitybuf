<?xml version="1.0" encoding="UTF-8"?>
<!-- The two remaining families that nest city objects the way a building does:
     a bridge with a part, an installation and a construction element, and a
     tunnel with a part.

     Both are read by the building reader, parameterised: the properties are
     named after the module (brid:consistsOfBridgePart, tun:consistsOfTunnelPart)
     but everything about them — the recursion, the generated child ids, the
     boundedBy surfaces, the geometry-before-semantics order — is the same
     machinery.

     One member is one feature, whatever it holds: the bridge's three children
     and the tunnel's one are City Objects of their member's feature, sharing
     its vertex array.

     The bridge part carries geometry at two levels of detail — an LoD 1 cube
     and an LoD 2 surface — and its one WallSurface describes the LoD 2 one,
     naming the polygon by gml:id from the other side, as CityGML usually does.
     The tunnel part deliberately has no gml:id, so the generated
     "{parent}-part-{n}" name is exercised in this family too.

     Everything sits between (200, 300, 0) and (213, 301, 1); the pieces are
     spaced a metre apart in x so that no two of them share a vertex. -->
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:brid="http://www.opengis.net/citygml/bridge/2.0"
                xmlns:tun="http://www.opengis.net/citygml/tunnel/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
  <gml:boundedBy>
    <gml:Envelope srsName="EPSG:7415" srsDimension="3">
      <gml:lowerCorner>200 300 0</gml:lowerCorner>
      <gml:upperCorner>213 301 1</gml:upperCorner>
    </gml:Envelope>
  </gml:boundedBy>

  <core:cityObjectMember>
    <brid:Bridge gml:id="bridge-1">
      <gml:name>Foot bridge</gml:name>
      <brid:class>1000</brid:class>
      <brid:consistsOfBridgePart>
        <brid:BridgePart gml:id="bp-1">
          <brid:lod1Solid>
            <gml:Solid>
              <gml:exterior>
                <gml:CompositeSurface>
                  <!-- bottom, z = 0 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>200 300 0 201 300 0 201 301 0 200 301 0 200 300 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- top, z = 1 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>200 300 1 200 301 1 201 301 1 201 300 1 200 300 1</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- y = 300 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>200 300 0 200 300 1 201 300 1 201 300 0 200 300 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- y = 301 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>201 301 0 201 301 1 200 301 1 200 301 0 201 301 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- x = 200 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>200 301 0 200 301 1 200 300 1 200 300 0 200 301 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                  <!-- x = 201 -->
                  <gml:surfaceMember>
                    <gml:Polygon>
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>201 300 0 201 300 1 201 301 1 201 301 0 201 300 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                </gml:CompositeSurface>
              </gml:exterior>
            </gml:Solid>
          </brid:lod1Solid>
          <!-- The parapet: a boundary surface of the part, whose polygon the
               part's LoD 2 surface names by gml:id. -->
          <brid:boundedBy>
            <brid:WallSurface gml:id="bp-wall">
              <gml:name>North parapet</gml:name>
              <brid:lod2MultiSurface>
                <gml:MultiSurface>
                  <gml:surfaceMember>
                    <gml:Polygon gml:id="bp-wall-p">
                      <gml:exterior>
                        <gml:LinearRing>
                          <gml:posList>202 300 0 203 300 0 203 300 1 202 300 1 202 300 0</gml:posList>
                        </gml:LinearRing>
                      </gml:exterior>
                    </gml:Polygon>
                  </gml:surfaceMember>
                </gml:MultiSurface>
              </brid:lod2MultiSurface>
            </brid:WallSurface>
          </brid:boundedBy>
          <brid:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember xlink:href="#bp-wall-p"/>
            </gml:MultiSurface>
          </brid:lod2MultiSurface>
        </brid:BridgePart>
      </brid:consistsOfBridgePart>
      <brid:outerBridgeInstallation>
        <brid:BridgeInstallation gml:id="bi-1">
          <brid:lod2Geometry>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>204 300 0 205 300 0 205 301 0 204 300 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </brid:lod2Geometry>
        </brid:BridgeInstallation>
      </brid:outerBridgeInstallation>
      <!-- CityGML spells this element BridgeConstructionElement and CityJSON
           spells the same thing BridgeConstructiveElement. -->
      <brid:outerBridgeConstruction>
        <brid:BridgeConstructionElement gml:id="bce-1">
          <brid:lod2Geometry>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>206 300 0 207 300 0 207 301 0 206 300 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </brid:lod2Geometry>
        </brid:BridgeConstructionElement>
      </brid:outerBridgeConstruction>
    </brid:Bridge>
  </core:cityObjectMember>

  <core:cityObjectMember>
    <tun:Tunnel gml:id="tunnel-1">
      <gml:name>Road tunnel</gml:name>
      <tun:lod1Solid>
        <gml:Solid>
          <gml:exterior>
            <gml:CompositeSurface>
              <!-- bottom, z = 0 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>210 300 0 211 300 0 211 301 0 210 301 0 210 300 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- top, z = 1 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>210 300 1 210 301 1 211 301 1 211 300 1 210 300 1</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- y = 300 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>210 300 0 210 300 1 211 300 1 211 300 0 210 300 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- y = 301 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>211 301 0 211 301 1 210 301 1 210 301 0 211 301 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- x = 210 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>210 301 0 210 301 1 210 300 1 210 300 0 210 301 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
              <!-- x = 211 -->
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>211 300 0 211 300 1 211 301 1 211 301 0 211 300 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:CompositeSurface>
          </gml:exterior>
        </gml:Solid>
      </tun:lod1Solid>
      <tun:consistsOfTunnelPart>
        <!-- No gml:id: this part is named after its parent and its place. -->
        <tun:TunnelPart>
          <tun:lod2MultiSurface>
            <gml:MultiSurface>
              <gml:surfaceMember>
                <gml:Polygon>
                  <gml:exterior>
                    <gml:LinearRing>
                      <gml:posList>212 300 0 213 300 0 213 301 0 212 300 0</gml:posList>
                    </gml:LinearRing>
                  </gml:exterior>
                </gml:Polygon>
              </gml:surfaceMember>
            </gml:MultiSurface>
          </tun:lod2MultiSurface>
        </tun:TunnelPart>
      </tun:consistsOfTunnelPart>
    </tun:Tunnel>
  </core:cityObjectMember>
</core:CityModel>
