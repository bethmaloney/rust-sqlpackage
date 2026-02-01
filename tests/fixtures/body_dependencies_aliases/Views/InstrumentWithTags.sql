-- View with STUFF function containing nested subquery with table alias
-- This tests the bug where [ITTAG] alias is not resolved properly
-- Expected: [ITTAG] should resolve to [dbo].[Tag], not appear as [ITTAG].[Name]
CREATE VIEW [dbo].[InstrumentWithTags]
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
        STUFF((
            SELECT ', ' + [ITTAG].[Name]
            FROM [dbo].[InstrumentTag] [IT2]
            INNER JOIN [dbo].[Tag] [ITTAG] ON [IT2].TagId = [ITTAG].Id
            WHERE IT.InstrumentId = [IT2].InstrumentId
            FOR XML PATH('')
        ), 1, 2, '') AS TagList
    FROM [dbo].[InstrumentTag] IT
    GROUP BY IT.InstrumentId
) Tags ON Tags.InstrumentId = I.Id;
GO
