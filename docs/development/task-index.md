# Flokzy Task Index

Snapshot read on **2026-10-13** from the [Lavender project](https://app.flokzy.com/app/projects/3cf1d056-2bd8-4cb7-9381-c83d58fb1bb2). It contains 91 existing tasks. Flokzy holds current status, assignment, discussion and dependency links; read a task there before claiming it. This page is navigation, not another backlog.

The earlier readability pass refined 67 initially open descriptions in place.
This read-back contains 27 Done tasks and 64 open tasks. ENG-1012 and ENG-1014
record the earlier contributor/documentation work; this execution completed
ENG-949 and ENG-972. ENG-948 and ENG-952 remain In Progress with their verified
slices and remaining scope recorded in Flokzy. Completed history, ownership,
priorities, deadlines, parents and links were preserved. Large workstream/wave
tickets are tracking parents; use their existing children for focused work.
No duplicate project/task or cycle was created. The cycle list contained no
current/upcoming cycle, so the [roadmap](../roadmap.md) uses milestones without
invented dates.

## Picking work

Start with the [roadmap's next tasks](../roadmap.md#what-to-work-on-next). “All recorded blockers done” means task links allow a start; keys, source terms, platform prerequisites and comments can still matter. Coordinate with the people already working on In Progress or In Review tasks.

## Shared foundations and development

| Task | Deliverable | Status at snapshot | Recorded unfinished blockers |
|---|---|---|---|
| [ENG-922](https://app.flokzy.com/app/tasks/f899a5e7-95a5-408a-bbd6-73031c262090) | Confirm product scope and owner decisions | Todo | None recorded |
| [ENG-923](https://app.flokzy.com/app/tasks/e6e42be1-f609-4ec6-bb33-772e6e4a2842) | Maintain the Rust workspace, contracts and geospatial foundations | Todo | None recorded |
| [ENG-925](https://app.flokzy.com/app/tasks/7140c7cc-8359-4c46-928c-8b195fbc8a53) | Keep one reusable core for web and desktop | Todo | None recorded |
| [ENG-926](https://app.flokzy.com/app/tasks/dec0b319-9c7b-47be-9d90-07afd3681afa) | Complete the shared frontend foundations | Todo | None recorded |
| [ENG-928](https://app.flokzy.com/app/tasks/cef57082-aec1-4bde-916d-64ea45d30e88) | Maintain the shared design system | Todo | None recorded |
| [ENG-936](https://app.flokzy.com/app/tasks/dd8947c7-1549-408a-979c-0b622da9a6f4) | Audit the old app (God's Eye View) and write Lavender's full set of specifications | Done | — |
| [ENG-937](https://app.flokzy.com/app/tasks/3192973d-2ad6-458a-90e2-c80c4db6adf8) | Prove the 3D globe works inside the macOS desktop app under strict security rules | Done | — |
| [ENG-938](https://app.flokzy.com/app/tasks/07c4bcf2-6328-466b-81f0-712f71514a96) | Record owner decisions on licences, branding and hosting | Todo | None recorded |
| [ENG-939](https://app.flokzy.com/app/tasks/5e515bc7-8ffb-4714-a100-f3c91ce28198) | Set up Lavender's Rust project with code-quality and dependency-licence checks | Done | — |
| [ENG-940](https://app.flokzy.com/app/tasks/8a8dbda2-2494-45b0-a464-7b9db2a2df35) | Define one set of data formats shared by the Rust backend and the web interface | Done | — |
| [ENG-941](https://app.flokzy.com/app/tasks/2e1a729d-c255-4422-aa83-6b906fcf29b2) | Build the shared map maths: distances, map areas, coordinate formats and orbits | Done | — |
| [ENG-942](https://app.flokzy.com/app/tasks/a3515564-450f-462c-bf64-9a5b1d6d24f9) | Send every outside data request through one safe, rule-enforcing client | Done | — |
| [ENG-943](https://app.flokzy.com/app/tasks/0b243395-3c37-4045-8260-d273b9693dc9) | Fetch each live feed once and share it with everyone watching it | Done | — |
| [ENG-944](https://app.flokzy.com/app/tasks/4f6ba068-b4bf-4825-8deb-410555c606a8) | Make data licences enforceable: a source registry, licence mode and source health | Done | — |
| [ENG-945](https://app.flokzy.com/app/tasks/d2b36ead-36b2-4462-80c9-59b19c99bbef) | Give the web server and the desktop app one shared entry point into the backend | Done | — |
| [ENG-969](https://app.flokzy.com/app/tasks/f2f7ef54-be61-46a7-a65e-26ce5a20a40c) | Set up the web interface project with its build, test and lint tools | Done | — |
| [ENG-970](https://app.flokzy.com/app/tasks/9fe93c9f-f25e-4737-a110-ddb64b253963) | Let the interface talk to a "platform", so the same screens run on web and desktop | Done | — |
| [ENG-971](https://app.flokzy.com/app/tasks/c2e9bd40-7fca-40b5-b24e-36fc94ed275e) | Keep interface state predictable and feed live data to the map without slowing screens | Done | — |
| [ENG-972](https://app.flokzy.com/app/tasks/c4f722a9-91a4-4b2c-833b-4781f1800586) | Share and restore a workspace view | Done | — |
| [ENG-981](https://app.flokzy.com/app/tasks/dcb27ac9-0500-47c7-a439-e5dbdfc39dbf) | Put the design system into code: colours, fonts, rounded corners and motion | Done | — |
| [ENG-982](https://app.flokzy.com/app/tasks/e3f2261e-3446-4f1c-8a30-aabc554c68a5) | Build the basic interface components, tested for keyboard and screen-reader use | Done | — |
| [ENG-1010](https://app.flokzy.com/app/tasks/3918780a-4186-4d3b-b79c-2c1d7a822ac7) | Fix the Rust backend review findings: task leak, rate-limit bypass, stream limits, parsing | Done | — |
| [ENG-1012](https://app.flokzy.com/app/tasks/3dcd0f8c-eca5-48c1-948e-657d94277e12) | Simplify Rust modules and document contributor workflows | Done | — |
| [ENG-1014](https://app.flokzy.com/app/tasks/53919562-da10-4d7f-9766-b86922e2a6ad) | Build a Lavender-styled browser documentation experience | Done | — |

## Providers and globe rendering

| Task | Deliverable | Status at snapshot | Recorded unfinished blockers |
|---|---|---|---|
| [ENG-924](https://app.flokzy.com/app/tasks/940526ef-e908-4ab2-8bad-17435c82f9ab) | Move the remaining data providers to Rust | Todo | None recorded |
| [ENG-927](https://app.flokzy.com/app/tasks/46e15deb-e561-4773-a62f-0956fbb204f3) | Complete the live globe layers | Todo | None recorded |
| [ENG-946](https://app.flokzy.com/app/tasks/fed86854-8746-4864-823a-0ae0255f4f93) | Run Lavender's API, live stream and web interface from one secure Rust server | Done | — |
| [ENG-947](https://app.flokzy.com/app/tasks/dd14121d-4646-48b4-8959-3ffcaaf14101) | Serve earthquakes from the USGS through the Rust backend | Done | — |
| [ENG-948](https://app.flokzy.com/app/tasks/328b7b6a-0312-4801-ac63-c688bd3dc462) | Complete the aircraft feed, history and optional sources | In Progress | None recorded |
| [ENG-949](https://app.flokzy.com/app/tasks/3178f6a8-d432-4b16-953e-20681548690a) | Complete satellite tracks, passes and validation | Done | — |
| [ENG-950](https://app.flokzy.com/app/tasks/3f879345-de9b-4766-9d1f-87820918cdaf) | Add NASA FIRMS fire detections and map aggregation | Todo | None recorded |
| [ENG-951](https://app.flokzy.com/app/tasks/7ad4d567-da74-4ca3-ace7-3f112d5c6d87) | Add tropical cyclone tracks and forecast cones | Todo | None recorded |
| [ENG-952](https://app.flokzy.com/app/tasks/5489fdbe-74b7-401a-982d-a4ca22e163c7) | Complete place and live-entity search | In Progress | None recorded |
| [ENG-953](https://app.flokzy.com/app/tasks/98f08784-0686-4a05-99e1-188b88b10291) | Add a bounded live vessel store | Todo | None recorded |
| [ENG-954](https://app.flokzy.com/app/tasks/caa24064-b464-4f68-9b94-b99b33885a85) | Migrate weather imagery and lightning providers | Todo | None recorded |
| [ENG-955](https://app.flokzy.com/app/tasks/4651a15f-676e-4c01-90cd-cae4618548b2) | Decode and serve global wind forecasts in Rust | Todo | None recorded |
| [ENG-956](https://app.flokzy.com/app/tasks/ee895c6f-bc9d-4776-ad05-3e42a50841ae) | Add zoom-aware wildfire perimeters | Todo | None recorded |
| [ENG-957](https://app.flokzy.com/app/tasks/6c40bdde-a62a-4eba-8a89-d41053a00090) | Add launch data and satellite matching | Todo | None recorded |
| [ENG-958](https://app.flokzy.com/app/tasks/b3f870ff-1d82-4cc4-a149-7c711b26242d) | Serve static map context from Rust | Todo | None recorded |
| [ENG-959](https://app.flokzy.com/app/tasks/245282b0-3670-4913-b379-60e5627fbb1b) | Add bounded terrain-height queries | Todo | None recorded |
| [ENG-960](https://app.flokzy.com/app/tasks/0acda126-1216-4cda-97eb-28e8329b7943) | Add traffic flow with a daily request budget | Todo | None recorded |
| [ENG-961](https://app.flokzy.com/app/tasks/8160cbea-7b13-4d10-af9c-c6fffae48e03) | Add live transit vehicles and trails | Todo | None recorded |
| [ENG-962](https://app.flokzy.com/app/tasks/4acedab4-9ae4-46ff-9579-0aabae68e43a) | Add bike-share stations from approved systems | Todo | None recorded |
| [ENG-963](https://app.flokzy.com/app/tasks/39a9f58e-c6e0-42a4-a183-cdb9d592e49c) | Add paced routing with clear errors | Todo | None recorded |
| [ENG-964](https://app.flokzy.com/app/tasks/3462978b-a1b5-4f74-9fd7-dee4cd9a1e3a) | Migrate the optional public-camera layer | Todo | None recorded |
| [ENG-965](https://app.flokzy.com/app/tasks/86fce344-4dc4-4c52-9cd4-0ea01936181c) | Add Mapillary coverage with server-held credentials | Todo | None recorded |
| [ENG-966](https://app.flokzy.com/app/tasks/a91a414e-5bef-4712-8fcd-29181ed9f7e3) | Migrate safe map queries and military sites | Todo | None recorded |
| [ENG-967](https://app.flokzy.com/app/tasks/f6a325b0-f870-41cb-9e11-c79c9c4cb36b) | Migrate licensed cable data and regional briefs | Todo | None recorded |
| [ENG-968](https://app.flokzy.com/app/tasks/23bfd414-2202-4181-9afc-7518f0cd2134) | Add recent satellite-imagery search | Todo | None recorded |
| [ENG-973](https://app.flokzy.com/app/tasks/485f0f7c-9292-4733-912d-10637409f945) | Build the 3D globe engine: imagery, camera, clicking on things and error recovery | Done | — |
| [ENG-974](https://app.flokzy.com/app/tasks/9b05af8b-86d1-48db-abc6-f2070d638ea9) | Review aircraft rendering and update performance | In Review | None recorded |
| [ENG-975](https://app.flokzy.com/app/tasks/5b2f2a5a-f52c-4c28-91ac-1e2c26c5dec0) | Complete earthquake, fire and cyclone rendering | In Progress | None recorded |
| [ENG-976](https://app.flokzy.com/app/tasks/00e267ff-5488-4efa-9703-073b8b3be6ad) | Review browser satellite propagation and performance | In Review | None recorded |
| [ENG-977](https://app.flokzy.com/app/tasks/98be3459-cf84-475b-a8ba-1f46346bebf7) | Track and follow a selected entity | Todo | [ENG-974](https://app.flokzy.com/app/tasks/9b05af8b-86d1-48db-abc6-f2070d638ea9) |
| [ENG-978](https://app.flokzy.com/app/tasks/ceebeb15-0193-4530-be5e-cfaf868cc1f5) | Add authorised premium 3D imagery | Todo | None recorded |
| [ENG-979](https://app.flokzy.com/app/tasks/d9069d87-f16d-46de-ad70-00426d561861) | Keep dense globe labels readable | Todo | None recorded |
| [ENG-980](https://app.flokzy.com/app/tasks/282ac7f7-9764-4f3c-9702-66eb6f8d427a) | Render vessel positions and headings | Todo | [ENG-953](https://app.flokzy.com/app/tasks/98f08784-0686-4a05-99e1-188b88b10291) |

## Analyst workflows

| Task | Deliverable | Status at snapshot | Recorded unfinished blockers |
|---|---|---|---|
| [ENG-929](https://app.flokzy.com/app/tasks/d0808cf0-ecab-4750-bfd6-07a01d0f11c4) | Complete the analyst workspace | Todo | None recorded |
| [ENG-930](https://app.flokzy.com/app/tasks/4f9255b1-23dc-4167-95e4-5114c7143be1) | Restore the remaining upstream workflows | Todo | None recorded |
| [ENG-983](https://app.flokzy.com/app/tasks/5714f7de-e4bd-4a2f-97de-dd24672c6a24) | Complete the analyst shell review | In Review | None recorded |
| [ENG-984](https://app.flokzy.com/app/tasks/b80b490d-cf11-4131-8fbc-6980ca3d23c0) | Show every layer with its source, count and freshness at a glance | Done | — |
| [ENG-985](https://app.flokzy.com/app/tasks/651f8b08-b11b-4ebf-9d27-d645d0169f4e) | Show what any selected item is, where its data came from and how old it is | Done | — |
| [ENG-986](https://app.flokzy.com/app/tasks/b399a0de-69b3-440a-8afd-237334d0afea) | Find places, coordinates, live items, layers and actions from one search box | Done | — |
| [ENG-987](https://app.flokzy.com/app/tasks/1169df64-675b-4675-a6cb-fd1a85b54ccf) | Show every data source's credit, licence and health, generated from the registry | Done | — |
| [ENG-988](https://app.flokzy.com/app/tasks/c778c16b-5163-4cc7-a07b-5328fb71fa21) | Add preferences and safe desktop key entry | Todo | [ENG-983](https://app.flokzy.com/app/tasks/5714f7de-e4bd-4a2f-97de-dd24672c6a24) |
| [ENG-989](https://app.flokzy.com/app/tasks/7787f083-5295-434f-9f96-d4fa07e87d44) | Add persistent map annotations and measurements | Todo | None recorded |
| [ENG-990](https://app.flokzy.com/app/tasks/83679cf0-e688-448e-8281-8fb5b9c3f7f5) | Integrate the first wave of migrated layers | Todo | [ENG-953](https://app.flokzy.com/app/tasks/98f08784-0686-4a05-99e1-188b88b10291), [ENG-954](https://app.flokzy.com/app/tasks/caa24064-b464-4f68-9b94-b99b33885a85), [ENG-955](https://app.flokzy.com/app/tasks/4651a15f-676e-4c01-90cd-cae4618548b2), [ENG-956](https://app.flokzy.com/app/tasks/ee895c6f-bc9d-4776-ad05-3e42a50841ae), [ENG-957](https://app.flokzy.com/app/tasks/6c40bdde-a62a-4eba-8a89-d41053a00090), [ENG-958](https://app.flokzy.com/app/tasks/b3f870ff-1d82-4cc4-a149-7c711b26242d), [ENG-980](https://app.flokzy.com/app/tasks/282ac7f7-9764-4f3c-9702-66eb6f8d427a) |
| [ENG-991](https://app.flokzy.com/app/tasks/e1ae8777-bf43-431d-aa42-fc35f8e2247c) | Integrate the second wave of migrated layers | Todo | [ENG-960](https://app.flokzy.com/app/tasks/0acda126-1216-4cda-97eb-28e8329b7943), [ENG-961](https://app.flokzy.com/app/tasks/8160cbea-7b13-4d10-af9c-c6fffae48e03), [ENG-962](https://app.flokzy.com/app/tasks/4acedab4-9ae4-46ff-9579-0aabae68e43a), [ENG-963](https://app.flokzy.com/app/tasks/39a9f58e-c6e0-42a4-a183-cdb9d592e49c), [ENG-964](https://app.flokzy.com/app/tasks/3462978b-a1b5-4f74-9fd7-dee4cd9a1e3a), [ENG-965](https://app.flokzy.com/app/tasks/86fce344-4dc4-4c52-9cd4-0ea01936181c), [ENG-966](https://app.flokzy.com/app/tasks/a91a414e-5bef-4712-8fcd-29181ed9f7e3), [ENG-967](https://app.flokzy.com/app/tasks/f6a325b0-f870-41cb-9e11-c79c9c4cb36b), [ENG-968](https://app.flokzy.com/app/tasks/23bfd414-2202-4181-9afc-7518f0cd2134) |
| [ENG-992](https://app.flokzy.com/app/tasks/77ab2669-e09a-4485-89dd-fe0f3241762a) | Add authenticated MCP access to Lavender data | Todo | None recorded |
| [ENG-993](https://app.flokzy.com/app/tasks/24d7a7c7-4a73-4d07-a55a-f273af0e1bd8) | Add optional voice controls with spending limits | Todo | [ENG-992](https://app.flokzy.com/app/tasks/77ab2669-e09a-4485-89dd-fe0f3241762a) |
| [ENG-994](https://app.flokzy.com/app/tasks/8dda2bcb-44b7-4816-a198-689aa5b64856) | Plan the deferred radio, mission and event workflows | Todo | [ENG-990](https://app.flokzy.com/app/tasks/83679cf0-e688-448e-8281-8fb5b9c3f7f5) |

## Desktop

| Task | Deliverable | Status at snapshot | Recorded unfinished blockers |
|---|---|---|---|
| [ENG-931](https://app.flokzy.com/app/tasks/03dd9122-8030-4437-b0e3-7a8735711a2b) | Prepare the desktop app for release | Todo | None recorded |
| [ENG-995](https://app.flokzy.com/app/tasks/37d88622-1539-407a-bb6a-3c239c28c9c2) | Run the desktop app on the shared backend core, with the minimum permissions | Done | — |
| [ENG-996](https://app.flokzy.com/app/tasks/186ba4db-34e4-4c33-a0b4-37be96a08784) | Verify the desktop adapter and rendered live layer | In Progress | None recorded |
| [ENG-997](https://app.flokzy.com/app/tasks/0ea7e8c7-88bb-465e-8f8b-d1345bf21283) | Store desktop provider keys in the system keychain | Todo | None recorded |
| [ENG-998](https://app.flokzy.com/app/tasks/a67b6fcc-3e6e-4b97-8b4b-5e5a215b469d) | Verify desktop behaviour on Windows and Linux | Todo | [ENG-996](https://app.flokzy.com/app/tasks/186ba4db-34e4-4c33-a0b4-37be96a08784) |
| [ENG-999](https://app.flokzy.com/app/tasks/4656ec24-f1cd-46d1-99c9-11989700b6ee) | Package signed desktop releases and updates | Todo | [ENG-998](https://app.flokzy.com/app/tasks/a67b6fcc-3e6e-4b97-8b4b-5e5a215b469d), [ENG-938](https://app.flokzy.com/app/tasks/07c4bcf2-6328-466b-81f0-712f71514a96) |

## Measurement, security and CI

| Task | Deliverable | Status at snapshot | Recorded unfinished blockers |
|---|---|---|---|
| [ENG-932](https://app.flokzy.com/app/tasks/f8e62784-1d42-4340-ba44-00015542a219) | Measure and improve application performance | Todo | None recorded |
| [ENG-933](https://app.flokzy.com/app/tasks/c109e650-ba51-4321-b29a-47b0a86d567a) | Complete security, integration tests and CI | Todo | None recorded |
| [ENG-1000](https://app.flokzy.com/app/tasks/db841edb-fbd3-4317-998a-0a265952acdd) | Build a repeatable performance measurement harness | Todo | [ENG-974](https://app.flokzy.com/app/tasks/9b05af8b-86d1-48db-abc6-f2070d638ea9) |
| [ENG-1001](https://app.flokzy.com/app/tasks/f87a16c9-f58a-435b-8a34-dced5de637ab) | Meet the measured web and desktop performance targets | Todo | [ENG-990](https://app.flokzy.com/app/tasks/83679cf0-e688-448e-8281-8fb5b9c3f7f5), [ENG-1000](https://app.flokzy.com/app/tasks/db841edb-fbd3-4317-998a-0a265952acdd) |
| [ENG-1002](https://app.flokzy.com/app/tasks/ed0b7065-cb9f-416d-8120-fe79f24eec8a) | Complete automated security regression coverage | Todo | None recorded |
| [ENG-1003](https://app.flokzy.com/app/tasks/a9258462-ca62-47f6-8632-c075b9a097d9) | Review fixture-backed analyst journey tests | In Review | [ENG-974](https://app.flokzy.com/app/tasks/9b05af8b-86d1-48db-abc6-f2070d638ea9) |
| [ENG-1004](https://app.flokzy.com/app/tasks/3625e8eb-c541-499d-a58b-6d53a739f8ae) | Integrate Lavender into monorepo CI | Todo | None recorded |

## Guides and release

| Task | Deliverable | Status at snapshot | Recorded unfinished blockers |
|---|---|---|---|
| [ENG-934](https://app.flokzy.com/app/tasks/de1a657a-c517-47d8-9ba0-9b81311c8f67) | Write operator and analyst guides | Todo | None recorded |
| [ENG-935](https://app.flokzy.com/app/tasks/639cc3fe-ba14-4d52-b9ba-4fd43bcf2629) | Verify release readiness and retire legacy code | Todo | None recorded |
| [ENG-1005](https://app.flokzy.com/app/tasks/aa54b451-a641-459e-a865-3a15141f81de) | Write the deployment and operations guide | Todo | None recorded |
| [ENG-1006](https://app.flokzy.com/app/tasks/1e88e9a3-54df-4029-9768-4117bb214dc0) | Write the analyst user guide | Todo | None recorded |
| [ENG-1007](https://app.flokzy.com/app/tasks/aa40baa3-e131-4e3d-a7ff-aea771463bdd) | Deploy the web application over HTTPS | Todo | [ENG-938](https://app.flokzy.com/app/tasks/07c4bcf2-6328-466b-81f0-712f71514a96), [ENG-1005](https://app.flokzy.com/app/tasks/aa54b451-a641-459e-a865-3a15141f81de) |
| [ENG-1008](https://app.flokzy.com/app/tasks/2376be37-06bc-49eb-b97e-23a3ad78a44b) | Verify release licences and required notices | Todo | [ENG-938](https://app.flokzy.com/app/tasks/07c4bcf2-6328-466b-81f0-712f71514a96) |
| [ENG-1009](https://app.flokzy.com/app/tasks/73a8ae9e-5053-4afa-8f24-533d22b8f852) | Verify feature parity and retire legacy code | Todo | [ENG-990](https://app.flokzy.com/app/tasks/83679cf0-e688-448e-8281-8fb5b9c3f7f5), [ENG-991](https://app.flokzy.com/app/tasks/e1ae8777-bf43-431d-aa42-fc35f8e2247c), [ENG-1008](https://app.flokzy.com/app/tasks/2376be37-06bc-49eb-b97e-23a3ad78a44b) |

## Keeping the index useful

Update this dated snapshot when task states change. Add evidence to the existing Flokzy task, including commands and measurements; update [PROGRESS](../PROGRESS.md) when implementation or migration status changes. A completed basic feed does not imply all optional sources, history, release packaging or commercial permissions are finished.

