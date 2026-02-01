-- View in non-dbo schema with STUFF function containing nested subquery with UNQUALIFIED table names
-- This tests the bug where [ITTAG] alias is not resolved when:
-- 1. The view/procedure is in a non-dbo schema (e.g., [reporting])
-- 2. The JOIN in the nested subquery uses unqualified table names (e.g., "Tag" instead of "[dbo].[Tag]")
--
-- Expected: [ITTAG].[Name] should resolve to [dbo].[Tag].[Name]
-- Bug: The unqualified table name "Tag" is searched in [reporting] schema (the object's schema),
--      and when not found there, alias resolution fails. [ITTAG].[Name] is emitted as-is,
--      causing deployment to fail with "could not be resolved because no element with that name exists"
CREATE VIEW [reporting].[InstrumentWithTagsUnqualified]
AS
SELECT
    I.Id,
    I.Name,
    Tags.TagList
FROM [dbo].[Instrument] I
LEFT JOIN (
    SELECT
        IT.InstrumentId,
        -- STUFF function with nested SELECT using table alias [ITTAG]
        -- NOTE: Uses UNQUALIFIED table names (InstrumentTag, Tag) - this triggers the bug
        STUFF((
            SELECT ', ' + [ITTAG].[Name]
            FROM InstrumentTag [IT2]
            INNER JOIN Tag [ITTAG] ON [IT2].TagId = [ITTAG].Id
            WHERE IT.InstrumentId = [IT2].InstrumentId
            FOR XML PATH('')
        ), 1, 2, '') AS TagList
    FROM [dbo].[InstrumentTag] IT
    GROUP BY IT.InstrumentId
) Tags ON Tags.InstrumentId = I.Id;
GO
