-- Procedure with nested subquery containing table aliases
-- Tests alias resolution in procedure body dependencies
CREATE PROCEDURE [dbo].[GetAccountsWithTags]
    @MinTagCount INT = 1
AS
BEGIN
    SELECT
        A.Id,
        A.AccountNumber,
        TagData.AllTags
    FROM [dbo].[Account] A
    INNER JOIN (
        -- Nested subquery with STUFF and aliases
        SELECT
            AT1.AccountId,
            STUFF((
                SELECT ', ' + [TNAME].[Name]
                FROM [dbo].[AccountTag] [AT2]
                INNER JOIN [dbo].[Tag] [TNAME] ON [AT2].TagId = [TNAME].Id
                WHERE AT1.AccountId = [AT2].AccountId
                FOR XML PATH('')
            ), 1, 2, '') AS AllTags,
            COUNT(*) AS TagCount
        FROM [dbo].[AccountTag] AT1
        GROUP BY AT1.AccountId
        HAVING COUNT(*) >= @MinTagCount
    ) TagData ON TagData.AccountId = A.Id;
END;
GO
