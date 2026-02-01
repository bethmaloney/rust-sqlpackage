-- Procedure with DELETE...FROM containing aliases
-- Tests alias resolution in DELETE statements with FROM clause
CREATE PROCEDURE [dbo].[DeleteAccountWithFrom]
    @TagName NVARCHAR(100)
AS
BEGIN
    SET NOCOUNT ON;

    -- Delete account tags for accounts with specific tag
    DELETE [AT1]
    FROM [dbo].[AccountTag] [AT1]
    INNER JOIN [dbo].[Account] [A1] ON [A1].Id = [AT1].AccountId
    INNER JOIN [dbo].[Tag] [T1] ON [T1].Id = [AT1].TagId
    WHERE [T1].[Name] = @TagName
        AND [A1].Status = 'Inactive';
END;
GO
