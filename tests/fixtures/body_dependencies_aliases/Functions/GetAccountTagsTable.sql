-- Table-valued function with nested aliases
-- Tests alias resolution in TVF bodies
CREATE FUNCTION [dbo].[GetAccountTagsTable]
(
    @AccountId INT
)
RETURNS TABLE
AS
RETURN
(
    SELECT
        [ATAG].[Id] AS TagId,
        [ATAG].[Name] AS TagName,
        SubData.OtherAccountCount
    FROM [dbo].[AccountTag] [ATA]
    INNER JOIN [dbo].[Tag] [ATAG] ON [ATA].TagId = [ATAG].Id
    OUTER APPLY (
        -- Nested subquery with alias AT2
        SELECT COUNT(DISTINCT [AT2].AccountId) AS OtherAccountCount
        FROM [dbo].[AccountTag] [AT2]
        WHERE [AT2].TagId = [ATAG].Id
            AND [AT2].AccountId != @AccountId
    ) SubData
    WHERE [ATA].AccountId = @AccountId
);
GO
