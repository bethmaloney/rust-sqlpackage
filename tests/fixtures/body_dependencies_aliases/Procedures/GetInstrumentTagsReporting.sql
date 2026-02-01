-- Procedure in non-dbo schema with STUFF/FOR XML containing unqualified table references
-- This tests the schema resolution bug where unqualified tables in nested subqueries
-- are resolved to the containing object's schema instead of [dbo]
--
-- Bug: When this procedure is in [reporting] schema and references unqualified "Tag",
--      it gets resolved to [reporting].[Tag] which doesn't exist (tables are in [dbo])
CREATE PROCEDURE [reporting].[GetInstrumentTagsReporting]
    @InstrumentId INT
AS
BEGIN
    SET NOCOUNT ON;

    SELECT
        I.Id,
        I.Name,
        Tags.TagList
    FROM [dbo].[Instrument] I
    CROSS APPLY (
        SELECT
            -- STUFF with nested SELECT using UNQUALIFIED table names
            -- Bug: "Tag [ITTAG]" resolves to [reporting].[Tag] instead of [dbo].[Tag]
            STUFF((
                SELECT ', ' + [ITTAG].[Name]
                FROM InstrumentTag [IT2]
                INNER JOIN Tag [ITTAG] ON [IT2].TagId = [ITTAG].Id
                WHERE [IT2].InstrumentId = I.Id
                FOR XML PATH('')
            ), 1, 2, '') AS TagList
    ) Tags
    WHERE I.Id = @InstrumentId;
END;
GO
