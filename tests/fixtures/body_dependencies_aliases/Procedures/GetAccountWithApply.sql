CREATE PROCEDURE [dbo].[GetAccountWithApply]
AS
BEGIN
    SET NOCOUNT ON;

    SELECT
        a.Id,
        a.AccountNumber,
        d.TagCount
    FROM
        [dbo].[Account] a
    CROSS APPLY (
        SELECT COUNT(*) AS TagCount
        FROM [dbo].[AccountTag]
        WHERE AccountId = a.Id
    ) d
    WHERE a.Status = 1;

    SELECT
        a.Id,
        t.FirstTagName
    FROM
        [dbo].[Account] a
    OUTER APPLY (
        SELECT TOP 1 tag.[Name] AS FirstTagName
        FROM [dbo].[AccountTag] at
        INNER JOIN [dbo].[Tag] tag ON at.TagId = tag.Id
        WHERE at.AccountId = a.Id
        ORDER BY tag.[Name]
    ) t;
END;
GO
